use super::{home_dir, current_exe, write_service_file, remove_service_file, run_command, command_status, ServiceStatus, SERVICE_NAME};
use std::path::PathBuf;
use std::process::Command;

const LAUNCHD_LABEL: &str = "cc.kowx712.mmuvpn";
const LAUNCHD_TEMPLATE: &str = include_str!("../../templates/launchd.plist");

fn service_path() -> Result<PathBuf, String> {
    let mut path = home_dir()?;
    path.push("Library/LaunchAgents");
    path.push(format!("{LAUNCHD_LABEL}.plist"));
    Ok(path)
}

fn render_launchd_plist(exec: &std::path::Path) -> String {
    LAUNCHD_TEMPLATE.replace("{{EXEC_PATH}}", &exec.display().to_string())
}

pub fn enable_service() -> Result<(), String> {
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

pub fn disable_service() -> Result<(), String> {
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

pub fn probe_status() -> Result<ServiceStatus, String> {
    let path = service_path()?;
    let installed = path.exists();
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
    Ok(ServiceStatus {
        installed,
        enabled: loaded,
        running,
    })
}

#[cfg(test)]
mod tests {
    use super::render_launchd_plist;

    #[test]
    fn launchd_plist_mentions_binary_path() {
        let plist = render_launchd_plist(std::path::Path::new("/opt/homebrew/bin/mmuvpn"));
        assert!(plist.contains("<string>/opt/homebrew/bin/mmuvpn</string>"));
        assert!(plist.contains("cc.kowx712.mmuvpn"));
    }
}
