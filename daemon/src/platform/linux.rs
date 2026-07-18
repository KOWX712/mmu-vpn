use std::process::{Command, Stdio};
use crate::platform::{PlatformOps, DnsOps};
use super::common::openfortivpn_bin;

#[derive(Clone)]
pub struct LinuxPlatform;

impl LinuxPlatform {
    pub fn new() -> Self {
        Self
    }
}

impl PlatformOps for LinuxPlatform {
    fn vpn_start_command(&self, gateway_port: &str) -> Result<Command, String> {
        let mut cmd = Command::new("pkexec");
        cmd.arg(openfortivpn_bin());
        cmd.arg(gateway_port);
        cmd.arg("--saml-login");
        Ok(cmd)
    }

    fn vpn_stop(&self, child_pid: Option<u32>) -> Result<(), String> {
        use nix::sys::signal::{kill, Signal};
        use nix::unistd::Pid;

        // Try graceful child signal first if we have the PID
        if let Some(pid) = child_pid {
            let _ = kill(Pid::from_raw(pid as i32), Signal::SIGINT);
            
            // Wait a bit to see if it exits
            std::thread::sleep(std::time::Duration::from_millis(500));
            
            // Check if it's still running before escalating to pkexec
            if !crate::util::process_running("openfortivpn") {
                println!("[vpn] Process exited gracefully");
                return Ok(());
            }
        }

        // Only use pkexec if process is still running
        if crate::util::process_running("openfortivpn") {
            let mut pkill = Command::new("pkexec");
            pkill.args(["pkill", "-INT", "openfortivpn"]);
            let _ = pkill.stdout(Stdio::null()).stderr(Stdio::null()).status();
        }

        Ok(())
    }

    fn emergency_cleanup(&self) -> Result<(), String> {
        let mut cmd = Command::new("pkexec");
        cmd.args(["pkill", "-INT", "openfortivpn"]);
        let _ = cmd.stdout(Stdio::null()).stderr(Stdio::null()).status();
        Ok(())
    }

    fn platform_name(&self) -> &'static str {
        "Linux"
    }
}

impl DnsOps for LinuxPlatform {
    fn apply_vpn_dns(&mut self, _servers: &[String]) -> Result<(), String> {
        Ok(())
    }

    fn restore_dns(&mut self) -> Result<(), String> {
        Ok(())
    }

    fn cleanup_stale_dns(&self) -> Result<(), String> {
        Ok(())
    }

    fn needs_dns_management(&self) -> bool {
        false
    }
}
