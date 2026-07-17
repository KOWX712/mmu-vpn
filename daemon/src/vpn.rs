#[cfg(target_os = "macos")]
use std::fs;
use std::io::{BufRead, BufReader};
#[cfg(target_os = "macos")]
use std::io::Write;
#[cfg(target_os = "macos")]
use std::path::PathBuf;
#[cfg(target_os = "macos")]
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// Platform-specific executables (compile-time static binding via #[cfg]).
mod platform {
    #[cfg(target_os = "macos")]
    pub const SUDO: &str = "sudo";
    #[cfg(not(target_os = "macos"))]
    pub const SUDO: &str = "pkexec";

    #[cfg(target_os = "macos")]
    pub const OPEN: &str = "open";
    #[cfg(not(target_os = "macos"))]
    pub const OPEN: &str = "xdg-open";
}

const GATEWAY: &str = "vpnmlk.mmu.edu.my";
const PORT: &str = "443";

/// Survives tray crashes so DNS can still be restored later.
#[cfg(target_os = "macos")]
const DNS_BACKUP_PATH: &str = "/tmp/mmuvpn-dns-backup";

#[derive(Debug, PartialEq, Eq)]
struct CommandSpec {
    program: String,
    args: Vec<String>,
}

impl CommandSpec {
    fn new<P, I, S>(program: P, args: I) -> Self
    where
        P: Into<String>,
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            program: program.into(),
            args: args.into_iter().map(Into::into).collect(),
        }
    }
}

fn build_command(spec: &CommandSpec) -> Command {
    let mut command = Command::new(&spec.program);
    command.args(&spec.args);
    command
}

fn which_binary(name: &str) -> Option<String> {
    Command::new("which")
        .arg(name)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| {
            let path = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if path.is_empty() {
                None
            } else {
                Some(path)
            }
        })
}

fn openfortivpn_bin() -> String {
    which_binary("openfortivpn")
        .or_else(|| {
            [
                "/opt/homebrew/bin/openfortivpn",
                "/usr/local/bin/openfortivpn",
                "/usr/bin/openfortivpn",
            ]
            .into_iter()
            .find(|p| std::path::Path::new(p).exists())
            .map(str::to_string)
        })
        .unwrap_or_else(|| "openfortivpn".to_string())
}

fn start_vpn_command(gateway_port: &str) -> CommandSpec {
    CommandSpec::new(
        platform::SUDO,
        [
            openfortivpn_bin(),
            gateway_port.to_string(),
            "--saml-login".to_string(),
        ],
    )
}

fn open_url_command(url: &str) -> CommandSpec {
    CommandSpec::new(platform::OPEN, [url])
}

fn fallback_stop_command() -> CommandSpec {
    CommandSpec::new(platform::SUDO, ["pkill", "-INT", "openfortivpn"])
}

#[cfg(not(target_os = "macos"))]
fn signal_process_command(pid: u32) -> CommandSpec {
    CommandSpec::new("kill", ["-INT".to_string(), pid.to_string()])
}

/// Shared progress from the openfortivpn stdout reader.
#[derive(Default)]
struct TunnelWatch {
    up: AtomicBool,
    dns_servers: Mutex<Vec<String>>,
}

#[derive(Clone, PartialEq, Debug)]
pub enum State {
    Disconnected,
    Connecting,
    Connected,
    Error(String),
}

impl State {
    pub fn label(&self) -> String {
        match self {
            State::Disconnected => "Disconnected".to_string(),
            State::Connecting => "Connecting".to_string(),
            State::Connected => "Connected".to_string(),
            State::Error(msg) => format!("Error: {}", msg),
        }
    }

    pub fn is_active(&self) -> bool {
        matches!(self, State::Connecting | State::Connected)
    }
}

/// Previous macOS DNS so we can restore on disconnect.
#[cfg(target_os = "macos")]
#[derive(Clone, Debug)]
struct SavedDns {
    service: String,
    /// `None` means DHCP / Empty (no manual servers).
    servers: Option<Vec<String>>,
}

pub struct VpnDaemon {
    pub state: State,
    child: Option<Child>,
    last_check: Instant,
    tunnel: Arc<TunnelWatch>,
    #[cfg(target_os = "macos")]
    dns_applied: bool,
    connect_start: Option<Instant>,
    last_health_check: Instant,
    on_campus: bool,
}

impl VpnDaemon {
    pub fn new() -> Self {
        Self {
            state: State::Disconnected,
            child: None,
            last_check: Instant::now(),
            tunnel: Arc::new(TunnelWatch::default()),
            #[cfg(target_os = "macos")]
            dns_applied: false,
            connect_start: None,
            last_health_check: Instant::now(),
            on_campus: false,
        }
    }

    /// Call when the tray starts: fix leftover DNS if VPN is already down.
    pub fn bootstrap_cleanup(&mut self) {
        #[cfg(target_os = "macos")]
        {
            if !ppp_tunnel_present() && !openfortivpn_running() {
                if let Err(e) = restore_macos_dns_from_backup() {
                    // Only log if there was something to restore.
                    if Path::new(DNS_BACKUP_PATH).exists() {
                        eprintln!("[vpn] Startup DNS cleanup failed: {}", e);
                    }
                }
            }
        }
    }

    pub fn start(&mut self) {
        if self.state.is_active() {
            return;
        }

        // Pre-check: detect if already on campus network
        self.on_campus = check_university_reachable();
        if self.on_campus {
            println!("[vpn] University resources already accessible (campus network detected)");
            println!("[vpn] VPN connection will proceed anyway");
        }

        self.state = State::Connecting;
        self.connect_start = Some(Instant::now());
        self.tunnel.up.store(false, Ordering::SeqCst);
        if let Ok(mut dns) = self.tunnel.dns_servers.lock() {
            dns.clear();
        }
        #[cfg(target_os = "macos")]
        {
            self.dns_applied = false;
        }

        let gateway_port = format!("{}:{}", GATEWAY, PORT);
        let start_command = start_vpn_command(&gateway_port);
        match build_command(&start_command)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(mut child) => {
                let stdout = child.stdout.take();
                let stderr = child.stderr.take();
                let tunnel = Arc::clone(&self.tunnel);

                thread::spawn(move || {
                    if let Some(stdout) = stdout {
                        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                            println!("[vpn] {}", line);
                            if line.contains("Authenticate at") {
                                if let Some(url) = extract_saml_url(&line) {
                                    println!("[vpn] Opening SAML URL: {}", url);
                                    if let Ok(mut child) =
                                        build_command(&open_url_command(&url)).spawn()
                                    {
                                        let _ = child.wait();
                                    }
                                }
                            }
                            if let Some(servers) = extract_vpn_nameservers(&line) {
                                println!("[vpn] VPN DNS servers: {:?}", servers);
                                if let Ok(mut dns) = tunnel.dns_servers.lock() {
                                    *dns = servers;
                                }
                            }
                            if line.contains("Tunnel is up") {
                                println!("[vpn] Tunnel is up");
                                tunnel.up.store(true, Ordering::SeqCst);
                            }
                        }
                    }
                });

                thread::spawn(move || {
                    if let Some(stderr) = stderr {
                        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                            eprintln!("[vpn] {}", line);
                        }
                    }
                });

                self.child = Some(child);
            }
            Err(e) => {
                eprintln!("Failed to start: {}", e);
                self.state = State::Disconnected;
            }
        }
    }

    pub fn stop(&mut self) {
        println!("[vpn] Stopping VPN...");

        // Get child PID before taking it
        #[cfg(not(target_os = "macos"))]
        let child_pid = self.child.as_ref().map(|c| c.id());

        // Signal child process first (synchronously to ensure it happens)
        #[cfg(not(target_os = "macos"))]
        if let Some(pid) = child_pid {
            println!("[vpn] Sending SIGINT to openfortivpn PID {}", pid);
            let _ = build_command(&signal_process_command(pid))
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }

        // Take child and wait in background thread
        if let Some(mut child) = self.child.take() {
            thread::spawn(move || {
                let deadline = Instant::now() + Duration::from_secs(3);
                loop {
                    if child.try_wait().map(|s| s.is_some()).unwrap_or(true) {
                        println!("[vpn] Child process exited");
                        break;
                    }
                    if Instant::now() >= deadline {
                        println!("[vpn] Child process timeout, killing");
                        let _ = child.kill();
                        let _ = child.wait();
                        break;
                    }
                    thread::sleep(Duration::from_millis(50));
                }
            });
        }

        // Give signal time to propagate before pkill
        thread::sleep(Duration::from_millis(200));

        // Always use pkill as backup to catch any orphaned processes
        println!("[vpn] Running pkill to ensure cleanup");
        let _ = build_command(&fallback_stop_command())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();

        #[cfg(target_os = "macos")]
        {
            if let Err(e) = restore_macos_dns_from_backup() {
                eprintln!("[vpn] DNS restore: {}", e);
            }
            self.dns_applied = false;
        }

        self.tunnel.up.store(false, Ordering::SeqCst);
        if let Ok(mut dns) = self.tunnel.dns_servers.lock() {
            dns.clear();
        }
        self.state = State::Disconnected;
        self.connect_start = None;
        self.on_campus = false;

        println!("[vpn] Stop complete");
    }

    pub fn check_alive(&mut self) {
        if self.last_check.elapsed() < Duration::from_millis(300) {
            return;
        }
        self.last_check = Instant::now();

        let running = openfortivpn_running();
        let tunnel_up = self.tunnel.up.load(Ordering::SeqCst);

        // Check for connection timeout (60 seconds in Connecting state)
        if matches!(self.state, State::Connecting) {
            if let Some(start) = self.connect_start {
                if start.elapsed() > Duration::from_secs(60) {
                    eprintln!("[vpn] Connection timeout - no tunnel after 60s");
                    self.state = State::Error("Connection timeout - check logs or run --cleanup".to_string());
                    self.connect_start = None;
                    if let Some(mut child) = self.child.take() {
                        let _ = child.kill();
                        let _ = child.wait();
                    }
                    let _ = build_command(&fallback_stop_command())
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .status();
                    return;
                }
            }
        }

        match (&self.state, running, tunnel_up) {
            (State::Connecting, true, true) => {
                // Verify connectivity before declaring success
                println!("[vpn] Tunnel reported up, verifying connectivity...");
                if check_university_reachable() {
                    #[cfg(target_os = "macos")]
                    self.apply_macos_dns_if_needed();
                    self.state = State::Connected;
                    self.connect_start = None;
                    self.last_health_check = Instant::now();
                    println!("[vpn] Connectivity verified - connected successfully");
                } else if !self.on_campus {
                    // Not on campus and can't reach university - this is an error
                    eprintln!("[vpn] Tunnel up but university resources unreachable");
                    self.state = State::Error("Tunnel up but no connectivity".to_string());
                    self.connect_start = None;
                }
                // If on_campus=true, resources were accessible before, so we can't verify VPN is working
                // In this case, trust the tunnel_up signal
            }
            (State::Connecting, true, false) => {
                // Still connecting, do nothing
            }
            (State::Connected, false, _) => {
                #[cfg(target_os = "macos")]
                {
                    if let Err(e) = restore_macos_dns_from_backup() {
                        eprintln!("[vpn] DNS restore after tunnel drop: {}", e);
                    }
                    self.dns_applied = false;
                }
                self.state = State::Disconnected;
                self.tunnel.up.store(false, Ordering::SeqCst);
                self.connect_start = None;
                if let Some(mut child) = self.child.take() {
                    let _ = child.wait();
                }
            }
            (State::Connected, true, true) => {
                // Periodic health check every 15 seconds
                if self.last_health_check.elapsed() > Duration::from_secs(15) {
                    self.last_health_check = Instant::now();
                    if !check_university_reachable() {
                        eprintln!("[vpn] Health check failed - university resources unreachable");
                        self.state = State::Error("Connection lost - university unreachable".to_string());
                    }
                }
                #[cfg(target_os = "macos")]
                self.apply_macos_dns_if_needed();
            }
            (State::Connecting, false, _) => {
                if let Some(ref mut child) = self.child {
                    if child.try_wait().map(|s| s.is_some()).unwrap_or(true) {
                        self.state = State::Error("openfortivpn process exited unexpectedly".to_string());
                        self.tunnel.up.store(false, Ordering::SeqCst);
                        self.connect_start = None;
                        if let Some(mut child) = self.child.take() {
                            let _ = child.wait();
                        }
                    }
                } else {
                    self.state = State::Disconnected;
                    self.tunnel.up.store(false, Ordering::SeqCst);
                    self.connect_start = None;
                }
            }
            (State::Disconnected, false, _) => {
                if let Some(mut child) = self.child.take() {
                    let _ = child.wait();
                }
            }
            (State::Error(_), _, _) => {
                // Stay in error state until user takes action
            }
            _ => {}
        }
    }

    #[cfg(target_os = "macos")]
    fn apply_macos_dns_if_needed(&mut self) {
        if self.dns_applied {
            return;
        }
        let servers = self
            .tunnel
            .dns_servers
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default();
        if servers.is_empty() {
            eprintln!("[vpn] Tunnel up but no VPN DNS servers parsed yet");
            return;
        }

        let Some(service) = primary_network_service() else {
            eprintln!("[vpn] Could not detect primary network service for DNS");
            return;
        };

        // Persist previous DNS *before* changing anything (survives crashes).
        if !Path::new(DNS_BACKUP_PATH).exists() {
            let saved = SavedDns {
                service: service.clone(),
                servers: get_dns_servers(&service),
            };
            if let Err(e) = write_dns_backup(&saved) {
                eprintln!("[vpn] Failed to write DNS backup: {}", e);
                return;
            }
        }

        match set_dns_servers(&service, Some(&servers)) {
            Ok(()) => {
                self.dns_applied = true;
                println!(
                    "[vpn] Applied macOS DNS on '{}': {:?}",
                    service, servers
                );
            }
            Err(e) => eprintln!("[vpn] Failed to apply macOS DNS: {}", e),
        }
    }
}

fn openfortivpn_running() -> bool {
    Command::new("pgrep")
        .arg("openfortivpn")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn check_university_reachable() -> bool {
    Command::new("ping")
        .args(["-c", "1", "-W", "2", "erep.mmu.edu.my"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Full cleanup usable without a running tray: stop VPN + restore DNS.
pub fn emergency_cleanup() {
    println!("Stopping openfortivpn and restoring DNS (if needed)...");

    #[cfg(target_os = "macos")]
    {
        match stop_vpn_and_restore_dns() {
            Ok(()) => println!("Cleanup finished."),
            Err(e) => {
                eprintln!("Combined cleanup failed ({e}); trying fallbacks...");
                let _ = build_command(&fallback_stop_command())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
                match restore_macos_dns_from_backup() {
                    Ok(true) => println!("Restored previous DNS settings."),
                    Ok(false) => {
                        if let Some(service) = primary_network_service() {
                            if looks_like_stuck_vpn_dns(&service) {
                                match set_dns_servers(&service, None) {
                                    Ok(()) => println!(
                                        "Cleared stuck DNS on '{}' (set to DHCP/Empty).",
                                        service
                                    ),
                                    Err(e) => eprintln!("Failed to clear DNS: {}", e),
                                }
                            } else {
                                println!("No DNS backup found; current DNS left unchanged.");
                            }
                        }
                    }
                    Err(e) => eprintln!("DNS restore failed: {}", e),
                }
            }
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = build_command(&fallback_stop_command())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        println!("VPN stop signal sent.");
    }
}

/// Parse: `Got addresses: [10.9.12.1], ns [10.242.3.201, 10.242.3.200]`
fn extract_vpn_nameservers(line: &str) -> Option<Vec<String>> {
    let marker = "ns [";
    let start = line.find(marker)? + marker.len();
    let end = line[start..].find(']')? + start;
    let body = line[start..end].trim();
    if body.is_empty() {
        return None;
    }
    let servers: Vec<String> = body
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if servers.is_empty() {
        None
    } else {
        Some(servers)
    }
}

fn extract_saml_url(line: &str) -> Option<String> {
    let start = line.find('\'')? + 1;
    let end = line[start..].find('\'')? + start;
    Some(line[start..end].to_string())
}

#[cfg(target_os = "macos")]
fn ppp_tunnel_present() -> bool {
    Command::new("ifconfig")
        .arg("ppp0")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(target_os = "macos")]
fn primary_network_service() -> Option<String> {
    let iface = default_interface()?;
    service_for_interface(&iface).or_else(|| Some("Wi-Fi".to_string()))
}

#[cfg(target_os = "macos")]
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

#[cfg(target_os = "macos")]
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

#[cfg(target_os = "macos")]
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

/// Heuristic: manual DNS still set to private RFC1918 addresses while VPN is down.
#[cfg(target_os = "macos")]
fn looks_like_stuck_vpn_dns(service: &str) -> bool {
    match get_dns_servers(service) {
        Some(servers) if !servers.is_empty() => servers.iter().all(|s| is_private_ipv4(s)),
        _ => false,
    }
}

#[cfg(target_os = "macos")]
fn is_private_ipv4(s: &str) -> bool {
    let parts: Vec<_> = s.split('.').collect();
    if parts.len() != 4 {
        return false;
    }
    let nums: Option<Vec<u8>> = parts.iter().map(|p| p.parse().ok()).collect();
    match nums.as_deref() {
        Some([10, ..]) => true,
        Some([172, b, ..]) if (16..=31).contains(b) => true,
        Some([192, 168, ..]) => true,
        _ => false,
    }
}

#[cfg(target_os = "macos")]
fn dns_backup_path() -> PathBuf {
    PathBuf::from(DNS_BACKUP_PATH)
}

#[cfg(target_os = "macos")]
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
    let mut f = fs::File::create(dns_backup_path()).map_err(|e| e.to_string())?;
    f.write_all(body.as_bytes()).map_err(|e| e.to_string())?;
    println!(
        "[vpn] Saved previous DNS for '{}' to {}",
        saved.service, DNS_BACKUP_PATH
    );
    Ok(())
}

#[cfg(target_os = "macos")]
fn read_dns_backup() -> Result<Option<SavedDns>, String> {
    let path = dns_backup_path();
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

/// Kill openfortivpn and restore DNS in a single admin authorization.
#[cfg(target_os = "macos")]
fn stop_vpn_and_restore_dns() -> Result<(), String> {
    let mut script = String::from(
        "/usr/bin/pkill -INT openfortivpn 2>/dev/null || true; /bin/sleep 1; ",
    );
    if let Some(saved) = read_dns_backup()? {
        script.push_str(&dns_change_shell(&saved.service, saved.servers.as_deref()));
        script.push_str("; ");
        script.push_str(DNS_FLUSH_SHELL);
        run_admin_shell(&script)?;
        let _ = fs::remove_file(dns_backup_path());
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
        // No backup: still stop VPN; clear stuck private DNS if present.
        if let Some(service) = primary_network_service() {
            if looks_like_stuck_vpn_dns(&service) {
                script.push_str(&dns_change_shell(&service, None));
                script.push_str("; ");
                script.push_str(DNS_FLUSH_SHELL);
                run_admin_shell(&script)?;
                println!(
                    "[vpn] Stopped VPN and cleared stuck DNS on '{}'",
                    service
                );
            } else {
                run_admin_shell(&script)?;
                println!("[vpn] Stopped VPN (no DNS backup to restore)");
            }
        } else {
            run_admin_shell(&script)?;
        }
    }
    Ok(())
}

/// Returns Ok(true) if restore ran, Ok(false) if nothing to restore.
#[cfg(target_os = "macos")]
fn restore_macos_dns_from_backup() -> Result<bool, String> {
    let Some(saved) = read_dns_backup()? else {
        return Ok(false);
    };
    set_dns_servers(&saved.service, saved.servers.as_deref())?;
    let _ = fs::remove_file(dns_backup_path());
    println!(
        "[vpn] Restored macOS DNS on '{}' to {:?}",
        saved.service,
        saved
            .servers
            .as_ref()
            .map(|s| s.as_slice())
            .unwrap_or(&[] as &[String])
    );
    Ok(true)
}

#[cfg(target_os = "macos")]
const DNS_FLUSH_SHELL: &str =
    "/usr/bin/dscacheutil -flushcache; /usr/bin/killall -HUP mDNSResponder || true";

#[cfg(target_os = "macos")]
fn dns_change_shell(service: &str, servers: Option<&[String]>) -> String {
    let mut ns_cmd = format!(
        "/usr/sbin/networksetup -setdnsservers {}",
        shell_single_quote(service)
    );
    match servers {
        Some(list) if !list.is_empty() => {
            for s in list {
                ns_cmd.push(' ');
                ns_cmd.push_str(&shell_single_quote(s));
            }
        }
        _ => {
            ns_cmd.push_str(" Empty");
        }
    }
    ns_cmd
}

/// One admin auth for setdnsservers + DNS cache flush (avoid double password prompts).
#[cfg(target_os = "macos")]
fn set_dns_servers(service: &str, servers: Option<&[String]>) -> Result<(), String> {
    let combined = format!(
        "{}; {}",
        dns_change_shell(service, servers),
        DNS_FLUSH_SHELL
    );

    // Prefer GUI admin prompt (works from menu bar without TTY). Fall back to sudo.
    if run_admin_shell(&combined).is_ok() {
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
    // sudo path: flush without a second interactive prompt if credentials are cached.
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

#[cfg(target_os = "macos")]
fn run_admin_shell(script: &str) -> Result<(), String> {
    // AppleScript: do shell script "..." with administrator privileges
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

#[cfg(target_os = "macos")]
fn shell_single_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_saml_url_from_openfortivpn_output() {
        let line = "[vpn] Authenticate at 'https://vpn.example.test/saml?token=abc'";

        assert_eq!(
            extract_saml_url(line),
            Some("https://vpn.example.test/saml?token=abc".to_string())
        );
    }

    #[test]
    fn returns_none_when_saml_url_is_missing() {
        assert_eq!(extract_saml_url("[vpn] Waiting for authentication"), None);
    }

    #[test]
    fn extracts_vpn_nameservers_from_openfortivpn_output() {
        let line = "INFO:   Got addresses: [10.9.12.1], ns [10.242.3.201, 10.242.3.200]";
        assert_eq!(
            extract_vpn_nameservers(line),
            Some(vec![
                "10.242.3.201".to_string(),
                "10.242.3.200".to_string()
            ])
        );
    }

    #[test]
    fn returns_none_when_nameservers_missing() {
        assert_eq!(extract_vpn_nameservers("INFO: Tunnel is up"), None);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn private_ipv4_detection() {
        assert!(is_private_ipv4("10.242.3.201"));
        assert!(is_private_ipv4("192.168.1.1"));
        assert!(!is_private_ipv4("8.8.8.8"));
        assert!(!is_private_ipv4("not-an-ip"));
    }

    #[test]
    fn start_vpn_uses_platform_sudo() {
        let start = start_vpn_command("vpnmlk.mmu.edu.my:443");
        assert_eq!(start.program, platform::SUDO);
        assert!(
            start.args[0].ends_with("openfortivpn") || start.args[0] == "openfortivpn",
            "expected openfortivpn path, got {}",
            start.args[0]
        );
        assert_eq!(start.args[1], "vpnmlk.mmu.edu.my:443");
        assert_eq!(start.args[2], "--saml-login");
    }

    #[test]
    fn open_url_uses_platform_open() {
        let browser = open_url_command("https://vpn.example.test/saml");
        assert_eq!(browser.program, platform::OPEN);
        assert_eq!(browser.args, vec!["https://vpn.example.test/saml"]);
    }

    #[test]
    fn fallback_stop_uses_platform_sudo() {
        let stop = fallback_stop_command();
        assert_eq!(stop.program, platform::SUDO);
        assert_eq!(stop.args, vec!["pkill", "-INT", "openfortivpn"]);
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn signal_command_targets_tracked_child_pid() {
        let cmd = signal_process_command(1234);

        assert_eq!(cmd.program, "kill");
        assert_eq!(cmd.args, vec!["-INT", "1234"]);
    }
}
