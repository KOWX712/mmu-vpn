#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;

use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

const SERVICE_NAME: &str = "mmuvpn";

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
use linux::{enable_service, disable_service, probe_status};

#[cfg(target_os = "macos")]
use macos::{enable_service, disable_service, probe_status};
