use std::process::Command;

pub trait PlatformOps {
    fn vpn_start_command(&self, gateway_port: &str) -> Result<Command, String>;
    fn vpn_stop(&self, child_pid: Option<u32>) -> Result<(), String>;
    fn emergency_cleanup(&self) -> Result<(), String>;
    fn platform_name(&self) -> &'static str;
}

pub trait DnsOps {
    fn apply_vpn_dns(&mut self, servers: &[String]) -> Result<(), String>;
    fn restore_dns(&mut self) -> Result<(), String>;
    fn cleanup_stale_dns(&self) -> Result<(), String>;
    fn needs_dns_management(&self) -> bool;
}

pub mod common;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::LinuxPlatform;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::MacOsPlatform;

#[cfg(target_os = "linux")]
pub type Platform = LinuxPlatform;

#[cfg(target_os = "macos")]
pub type Platform = MacOsPlatform;
