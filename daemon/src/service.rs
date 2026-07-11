use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

const SERVICE_NAME: &str = "mmuvpn";
#[cfg(target_os = "macos")]
const LAUNCHD_LABEL: &str = "cc.kowx712.mmuvpn";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceAction {
    Status,
    Enable,
    Disable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceStatus {
    pub installed: bool,
    pub enabled: bool,
    pub running: bool,
}

pub fn run(action: ServiceAction) -> Result<(), String> {
    match action {
        ServiceAction::Status => {
            println!("{}", describe_status(&probe_status()?));
            Ok(())
        }
        ServiceAction::Enable => enable_service(),
        ServiceAction::Disable => disable_service(),
    }
}

fn describe_status(status: &ServiceStatus) -> String {
    let installed = if status.installed {
        "installed"
    } else {
        "missing"
    };
    let enabled = if status.enabled {
        "enabled"
    } else {
        "disabled"
    };
    let running = if status.running { "running" } else { "stopped" };
    format!("service: {installed}, {enabled}, {running}")
}

fn home_dir() -> Result<PathBuf, String> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| "HOME is not set".to_string())
}

fn current_exe() -> Result<PathBuf, String> {
    env::current_exe().map_err(|e| e.to_string())
}

fn service_path() -> Result<PathBuf, String> {
    let mut path = home_dir()?;

    #[cfg(target_os = "linux")]
    {
        path.push(".config/systemd/user");
        path.push(format!("{SERVICE_NAME}.service"));
    }

    #[cfg(target_os = "macos")]
    {
        path.push("Library/LaunchAgents");
        path.push(format!("{LAUNCHD_LABEL}.plist"));
    }

    Ok(path)
}

fn write_service_file(path: &Path, body: &str) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }

    let mut file = fs::File::create(path).map_err(|e| e.to_string())?;
    file.write_all(body.as_bytes()).map_err(|e| e.to_string())?;

    #[cfg(unix)]
    fs::set_permissions(path, fs::Permissions::from_mode(0o644)).map_err(|e| e.to_string())?;

    Ok(())
}

fn remove_service_file(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err.to_string()),
    }
}

fn command_status(program: &str, args: &[&str]) -> Result<bool, String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    Ok(output.status.success())
}

fn run_command(program: &str, args: &[&str]) -> Result<(), String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

#[cfg(target_os = "linux")]
fn render_systemd_unit(exec: &Path) -> String {
    format!(
        "[Unit]\nDescription=MMU VPN Tray Daemon\nAfter=graphical-session.target\n\n[Service]\nType=simple\nExecStart={}\nRestart=on-failure\nRestartSec=5\n\n[Install]\nWantedBy=graphical-session.target\n",
        exec.display()
    )
}

#[cfg(target_os = "macos")]
fn render_launchd_plist(exec: &Path) -> String {
    let exec = exec.display();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>{LAUNCHD_LABEL}</string>
  <key>ProgramArguments</key>
  <array>
    <string>{exec}</string>
  </array>
  <key>EnvironmentVariables</key>
  <dict>
    <key>PATH</key>
    <string>/opt/homebrew/bin:/opt/homebrew/sbin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin</string>
  </dict>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <false/>
  <key>StandardOutPath</key>
  <string>/tmp/mmuvpn.log</string>
  <key>StandardErrorPath</key>
  <string>/tmp/mmuvpn.err</string>
</dict>
</plist>
"#
    )
}

#[cfg(target_os = "linux")]
fn enable_service() -> Result<(), String> {
    let path = service_path()?;
    write_service_file(&path, &render_systemd_unit(&current_exe()?))?;
    run_command("systemctl", &["--user", "daemon-reload"])?;
    run_command(
        "systemctl",
        &["--user", "enable", "--now", "mmuvpn.service"],
    )
}

#[cfg(target_os = "macos")]
fn enable_service() -> Result<(), String> {
    let path = service_path()?;
    write_service_file(&path, &render_launchd_plist(&current_exe()?))?;
    let uid = Command::new("id")
        .args(["-u"])
        .output()
        .map_err(|e| e.to_string())?;
    if !uid.status.success() {
        return Err(String::from_utf8_lossy(&uid.stderr).trim().to_string());
    }
    let user = String::from_utf8_lossy(&uid.stdout).trim().to_string();
    let domain = format!("gui/{user}");
    let path_str = path
        .to_str()
        .ok_or_else(|| "service path is not valid UTF-8".to_string())?;
    run_command("launchctl", &["bootstrap", &domain, path_str])
}

#[cfg(target_os = "linux")]
fn disable_service() -> Result<(), String> {
    let path = service_path()?;
    let disable = run_command(
        "systemctl",
        &["--user", "disable", "--now", "mmuvpn.service"],
    );
    let remove = remove_service_file(&path);
    let reload = run_command("systemctl", &["--user", "daemon-reload"]);
    disable.and(remove).and(reload)
}

#[cfg(target_os = "macos")]
fn disable_service() -> Result<(), String> {
    let path = service_path()?;
    let uid = Command::new("id")
        .args(["-u"])
        .output()
        .map_err(|e| e.to_string())?;
    if !uid.status.success() {
        return Err(String::from_utf8_lossy(&uid.stderr).trim().to_string());
    }
    let user = String::from_utf8_lossy(&uid.stdout).trim().to_string();
    let domain = format!("gui/{user}");
    let path_str = path
        .to_str()
        .ok_or_else(|| "service path is not valid UTF-8".to_string())?;
    let unload = run_command("launchctl", &["bootout", &domain, path_str]);
    let remove = remove_service_file(&path);
    unload.and(remove)
}

fn probe_status() -> Result<ServiceStatus, String> {
    let path = service_path()?;
    let installed = path.exists();

    #[cfg(target_os = "linux")]
    {
        let enabled = command_status("systemctl", &["--user", "is-enabled", "mmuvpn.service"])?;
        let running = command_status("systemctl", &["--user", "is-active", "mmuvpn.service"])?;
        return Ok(ServiceStatus {
            installed,
            enabled,
            running,
        });
    }

    #[cfg(target_os = "macos")]
    {
        let uid = Command::new("id")
            .args(["-u"])
            .output()
            .map_err(|e| e.to_string())?;
        if !uid.status.success() {
            return Err(String::from_utf8_lossy(&uid.stderr).trim().to_string());
        }
        let user = String::from_utf8_lossy(&uid.stdout).trim().to_string();
        let domain = format!("gui/{user}");
        let loaded = command_status("launchctl", &["print", &domain, LAUNCHD_LABEL])?;
        let running = command_status("pgrep", &["-x", SERVICE_NAME])?;
        return Ok(ServiceStatus {
            installed,
            enabled: loaded,
            running,
        });
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        return Err("service management is not supported on this platform".to_string());
    }
}

#[cfg(test)]
mod tests {
    #[cfg(target_os = "macos")]
    use super::render_launchd_plist;

    #[cfg(target_os = "linux")]
    use super::render_systemd_unit;

    #[cfg(target_os = "linux")]
    #[test]
    fn systemd_unit_mentions_binary_path() {
        let unit = render_systemd_unit(std::path::Path::new("/usr/bin/mmuvpn"));

        assert!(unit.contains("ExecStart=/usr/bin/mmuvpn"));
        assert!(unit.contains("WantedBy=graphical-session.target"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn launchd_plist_mentions_binary_path() {
        let plist = render_launchd_plist(std::path::Path::new("/opt/homebrew/bin/mmuvpn"));

        assert!(plist.contains("<string>/opt/homebrew/bin/mmuvpn</string>"));
        assert!(plist.contains("cc.kowx712.mmuvpn"));
    }
}
