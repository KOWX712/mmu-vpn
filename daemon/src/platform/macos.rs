use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::net::Ipv4Addr;
use crate::platform::{PlatformOps, DnsOps};
use crate::util::process_running;
use super::common::openfortivpn_bin;

const DNS_BACKUP_PATH: &str = "/tmp/mmuvpn-dns-backup";
const DNS_FLUSH_SHELL: &str =
    "/usr/bin/dscacheutil -flushcache; /usr/bin/killall -HUP mDNSResponder || true";
const ASKPASS_TEMPLATE: &str = include_str!("../../templates/askpass.sh");

#[derive(Clone, Debug)]
struct SavedDns {
    service: String,
    servers: Option<Vec<String>>,
}

#[derive(Clone)]
pub struct MacOsPlatform {
    dns_applied: bool,
}

impl MacOsPlatform {
    pub fn new() -> Self {
        Self { dns_applied: false }
    }

    fn sudo_askpass_path() -> String {
        let dir = std::env::var_os("HOME")
            .map(PathBuf::from)
            .map(|home| home.join("Library/Caches/mmuvpn"))
            .unwrap_or_else(|| std::env::temp_dir().join("mmuvpn"));
        dir.join("mmuvpn-askpass.sh").to_string_lossy().to_string()
    }

    fn ensure_sudo_askpass_helper() -> Result<(), String> {
        let path = PathBuf::from(Self::sudo_askpass_path());

        if path.exists() {
            if let Ok(metadata) = fs::metadata(&path) {
                let permissions = metadata.permissions();
                // Check for exactly 0o700, not just owner bits set
                if permissions.mode() & 0o777 == 0o700 {
                    return Ok(());
                }
            }
        }

        let Some(dir) = path.parent() else {
            return Err("Invalid askpass helper path".to_string());
        };

        fs::create_dir_all(&dir)
            .map_err(|e| format!("Failed to create sudo askpass helper dir: {}", e))?;

        fs::write(&path, ASKPASS_TEMPLATE)
            .map_err(|e| format!("Failed to write sudo askpass helper: {}", e))?;

        fs::set_permissions(&path, fs::Permissions::from_mode(0o700))
            .map_err(|e| format!("Failed to make sudo askpass helper executable: {}", e))?;

        Ok(())
    }

    fn ppp_tunnel_present() -> bool {
        Command::new("ifconfig")
            .arg("ppp0")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn primary_network_service() -> Option<String> {
        let iface = Self::default_interface()?;
        Self::service_for_interface(&iface).or_else(|| Some("Wi-Fi".to_string()))
    }

    fn default_interface() -> Option<String> {
        let output = Command::new("route")
            .args(["-n", "get", "default"])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&output.stdout);
        for line in text.lines() {
            let line = line.trim();
            if let Some(iface) = line.strip_prefix("interface:") {
                let iface = iface.trim();
                if !iface.is_empty() {
                    return Some(iface.to_string());
                }
            }
        }
        None
    }

    fn service_for_interface(iface: &str) -> Option<String> {
        let output = Command::new("networksetup")
            .arg("-listallhardwareports")
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&output.stdout);
        let mut current_port: Option<String> = None;
        for line in text.lines() {
            if let Some(port) = line.strip_prefix("Hardware Port: ") {
                current_port = Some(port.trim().to_string());
            } else if let Some(device) = line.strip_prefix("Device: ") {
                if device.trim() == iface {
                    return current_port;
                }
            }
        }
        None
    }

    fn get_dns_servers(service: &str) -> Option<Vec<String>> {
        let output = Command::new("networksetup")
            .args(["-getdnsservers", service])
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&output.stdout);
        let trimmed = text.trim();
        if trimmed.is_empty() || trimmed.contains("aren't any DNS Servers") {
            return None;
        }
        let servers: Vec<String> = trimmed
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_string)
            .collect();
        if servers.is_empty() {
            None
        } else {
            Some(servers)
        }
    }

    fn looks_like_stuck_vpn_dns(service: &str) -> bool {
        match Self::get_dns_servers(service) {
            Some(servers) if !servers.is_empty() => servers.iter().all(|s| Self::is_private_ipv4(s)),
            _ => false,
        }
    }

    fn is_private_ipv4(s: &str) -> bool {
        s.parse::<Ipv4Addr>()
            .ok()
            .map(|ip| ip.is_private())
            .unwrap_or(false)
    }

    fn dns_backup_path() -> PathBuf {
        PathBuf::from(DNS_BACKUP_PATH)
    }

    fn write_dns_backup(saved: &SavedDns) -> Result<(), String> {
        let mut body = String::new();
        body.push_str(&saved.service);
        body.push('\n');
        match &saved.servers {
            None => body.push_str("Empty\n"),
            Some(list) => {
                for s in list {
                    body.push_str(s);
                    body.push('\n');
                }
            }
        }
        let mut f = fs::File::create(Self::dns_backup_path()).map_err(|e| e.to_string())?;
        f.write_all(body.as_bytes()).map_err(|e| e.to_string())?;
        println!(
            "[vpn] Saved previous DNS for '{}' to {}",
            saved.service, DNS_BACKUP_PATH
        );
        Ok(())
    }

    fn read_dns_backup() -> Result<Option<SavedDns>, String> {
        let path = Self::dns_backup_path();
        if !path.exists() {
            return Ok(None);
        }
        let text = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty());
        let service = lines
            .next()
            .ok_or_else(|| "DNS backup missing service name".to_string())?
            .to_string();
        let rest: Vec<String> = lines.map(str::to_string).collect();
        let servers = if rest.is_empty() || rest == ["Empty"] {
            None
        } else {
            Some(rest)
        };
        Ok(Some(SavedDns { service, servers }))
    }

    fn dns_change_shell(service: &str, servers: Option<&[String]>) -> String {
        let mut ns_cmd = format!(
            "/usr/sbin/networksetup -setdnsservers {}",
            Self::shell_single_quote(service)
        );
        match servers {
            Some(list) if !list.is_empty() => {
                for s in list {
                    ns_cmd.push(' ');
                    ns_cmd.push_str(&Self::shell_single_quote(s));
                }
            }
            _ => {
                ns_cmd.push_str(" Empty");
            }
        }
        ns_cmd
    }

    fn set_dns_servers(service: &str, servers: Option<&[String]>) -> Result<(), String> {
        let combined = format!(
            "{}; {}",
            Self::dns_change_shell(service, servers),
            DNS_FLUSH_SHELL
        );

        if Self::run_admin_shell(&combined).is_ok() {
            return Ok(());
        }

        let mut args = vec![
            "networksetup".to_string(),
            "-setdnsservers".to_string(),
            service.to_string(),
        ];
        match servers {
            Some(list) if !list.is_empty() => args.extend(list.iter().cloned()),
            _ => args.push("Empty".to_string()),
        }
        let output = Command::new("sudo")
            .args(&args)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| e.to_string())?;
        if !output.status.success() {
            return Err(format!(
                "networksetup failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        let _ = Command::new("sudo")
            .args(["dscacheutil", "-flushcache"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        let _ = Command::new("sudo")
            .args(["killall", "-HUP", "mDNSResponder"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        Ok(())
    }

    fn run_admin_shell(script: &str) -> Result<(), String> {
        let escaped = script.replace('\\', "\\\\").replace('"', "\\\"");
        let source = format!(
            "do shell script \"{}\" with administrator privileges",
            escaped
        );
        let output = Command::new("osascript")
            .args(["-e", &source])
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| e.to_string())?;
        if output.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
        }
    }

    fn shell_single_quote(s: &str) -> String {
        format!("'{}'", s.replace('\'', "'\\''"))
    }

    fn stop_vpn_and_restore_dns() -> Result<(), String> {
        // Check if openfortivpn is running first to avoid unnecessary sleep
        let vpn_running = crate::util::process_running("openfortivpn");
        
        let mut script = String::new();
        if vpn_running {
            script.push_str("/usr/bin/pkill -INT openfortivpn 2>/dev/null || true; /bin/sleep 1; ");
        }
        
        if let Some(saved) = Self::read_dns_backup()? {
            script.push_str(&Self::dns_change_shell(&saved.service, saved.servers.as_deref()));
            script.push_str("; ");
            script.push_str(DNS_FLUSH_SHELL);
            Self::run_admin_shell(&script)?;
            let _ = fs::remove_file(Self::dns_backup_path());
            println!(
                "[vpn] Stopped VPN and restored DNS on '{}' to {:?}",
                saved.service,
                saved
                    .servers
                    .as_ref()
                    .map(|s| s.as_slice())
                    .unwrap_or(&[] as &[String])
            );
        } else {
            if let Some(service) = Self::primary_network_service() {
                if Self::looks_like_stuck_vpn_dns(&service) {
                    script.push_str(&Self::dns_change_shell(&service, None));
                    script.push_str("; ");
                    script.push_str(DNS_FLUSH_SHELL);
                    Self::run_admin_shell(&script)?;
                    println!("[vpn] Stopped VPN and cleared stuck DNS on '{}'", service);
                } else {
                    Self::run_admin_shell(&script)?;
                    println!("[vpn] Stopped VPN (no DNS backup to restore)");
                }
            } else {
                Self::run_admin_shell(&script)?;
            }
        }
        Ok(())
    }
}

impl PlatformOps for MacOsPlatform {
    fn vpn_start_command(&self, gateway_port: &str) -> Result<Command, String> {
        Self::ensure_sudo_askpass_helper()?;

        let mut cmd = Command::new("sudo");
        cmd.arg("-A");
        cmd.arg(openfortivpn_bin());
        cmd.arg(gateway_port);
        cmd.arg("--saml-login");
        cmd.env("SUDO_ASKPASS", Self::sudo_askpass_path());
        Ok(cmd)
    }

    fn vpn_stop(&self, _child_pid: Option<u32>) -> Result<(), String> {
        std::thread::sleep(std::time::Duration::from_millis(200));

        Self::ensure_sudo_askpass_helper()?;
        let mut cmd = Command::new("sudo");
        cmd.args(["-A", "pkill", "-INT", "openfortivpn"]);
        cmd.env("SUDO_ASKPASS", Self::sudo_askpass_path());
        let _ = cmd.stdout(Stdio::null()).stderr(Stdio::null()).status();

        Ok(())
    }

    fn emergency_cleanup(&self) -> Result<(), String> {
        match Self::stop_vpn_and_restore_dns() {
            Ok(()) => {
                println!("Cleanup finished.");
                Ok(())
            }
            Err(e) => {
                eprintln!("Combined cleanup failed ({e}); trying fallbacks...");
                Self::ensure_sudo_askpass_helper()?;
                let mut cmd = Command::new("sudo");
                cmd.args(["-A", "pkill", "-INT", "openfortivpn"]);
                cmd.env("SUDO_ASKPASS", Self::sudo_askpass_path());
                let _ = cmd.stdout(Stdio::null()).stderr(Stdio::null()).status();

                match Self::read_dns_backup()? {
                    Some(saved) => {
                        Self::set_dns_servers(&saved.service, saved.servers.as_deref())?;
                        let _ = fs::remove_file(Self::dns_backup_path());
                        println!("Restored previous DNS settings.");
                    }
                    None => {
                        if let Some(service) = Self::primary_network_service() {
                            if Self::looks_like_stuck_vpn_dns(&service) {
                                Self::set_dns_servers(&service, None)?;
                                println!("Cleared stuck DNS on '{}' (set to DHCP/Empty).", service);
                            } else {
                                println!("No DNS backup found; current DNS left unchanged.");
                            }
                        }
                    }
                }
                Ok(())
            }
        }
    }

    fn platform_name(&self) -> &'static str {
        "macOS"
    }
}

impl DnsOps for MacOsPlatform {
    fn apply_vpn_dns(&mut self, servers: &[String]) -> Result<(), String> {
        if self.dns_applied || servers.is_empty() {
            return Ok(());
        }

        let Some(service) = Self::primary_network_service() else {
            return Err("Could not detect primary network service for DNS".to_string());
        };

        if !Path::new(DNS_BACKUP_PATH).exists() {
            let saved = SavedDns {
                service: service.clone(),
                servers: Self::get_dns_servers(&service),
            };
            Self::write_dns_backup(&saved)?;
        }

        Self::set_dns_servers(&service, Some(servers))?;
        self.dns_applied = true;
        println!("[vpn] Applied macOS DNS on '{}': {:?}", service, servers);
        Ok(())
    }

    fn restore_dns(&mut self) -> Result<(), String> {
        if !self.dns_applied {
            return Ok(());
        }

        match Self::read_dns_backup()? {
            Some(saved) => {
                Self::set_dns_servers(&saved.service, saved.servers.as_deref())?;
                let _ = fs::remove_file(Self::dns_backup_path());
                println!(
                    "[vpn] Restored macOS DNS on '{}' to {:?}",
                    saved.service,
                    saved
                        .servers
                        .as_ref()
                        .map(|s| s.as_slice())
                        .unwrap_or(&[] as &[String])
                );
                self.dns_applied = false;
                Ok(())
            }
            None => {
                self.dns_applied = false;
                Ok(())
            }
        }
    }

    fn cleanup_stale_dns(&self) -> Result<(), String> {
        if !Self::ppp_tunnel_present() && !process_running("openfortivpn") {
            match Self::read_dns_backup()? {
                Some(saved) => {
                    Self::set_dns_servers(&saved.service, saved.servers.as_deref())?;
                    let _ = fs::remove_file(Self::dns_backup_path());
                }
                None => {}
            }
        }
        Ok(())
    }

    fn needs_dns_management(&self) -> bool {
        true
    }
}
