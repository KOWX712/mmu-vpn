use super::{home_dir, current_exe, write_service_file, remove_service_file, run_command, command_status, ServiceStatus, SERVICE_NAME};
use std::path::PathBuf;

const SYSTEMD_TEMPLATE: &str = include_str!("../../templates/systemd.service");

fn service_path() -> Result<PathBuf, String> {
    let mut path = home_dir()?;
    path.push(".config/systemd/user");
    path.push(format!("{SERVICE_NAME}.service"));
    Ok(path)
}

fn render_systemd_unit(exec: &std::path::Path) -> String {
    SYSTEMD_TEMPLATE.replace("{{EXEC_PATH}}", &exec.display().to_string())
}

pub fn enable_service() -> Result<(), String> {
    let path = service_path()?;
    write_service_file(&path, &render_systemd_unit(&current_exe()?))?;
    run_command("systemctl", &["--user", "daemon-reload"])?;
    run_command(
        "systemctl",
        &["--user", "enable", "--now", "mmuvpn.service"],
    )
}

pub fn disable_service() -> Result<(), String> {
    let path = service_path()?;
    let disable = run_command(
        "systemctl",
        &["--user", "disable", "--now", "mmuvpn.service"],
    );
    let remove = remove_service_file(&path);
    let reload = run_command("systemctl", &["--user", "daemon-reload"]);
    disable.and(remove).and(reload)
}

pub fn probe_status() -> Result<ServiceStatus, String> {
    let path = service_path()?;
    let installed = path.exists();
    let enabled = command_status("systemctl", &["--user", "is-enabled", "mmuvpn.service"])?;
    let running = command_status("systemctl", &["--user", "is-active", "mmuvpn.service"])?;
    Ok(ServiceStatus {
        installed,
        enabled,
        running,
    })
}

#[cfg(test)]
mod tests {
    use super::render_systemd_unit;

    #[test]
    fn systemd_unit_mentions_binary_path() {
        let unit = render_systemd_unit(std::path::Path::new("/usr/bin/mmuvpn"));
        assert!(unit.contains("ExecStart=/usr/bin/mmuvpn"));
        assert!(unit.contains("WantedBy=graphical-session.target"));
    }
}
