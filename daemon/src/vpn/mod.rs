pub mod connection;
mod platform_impl;

pub use platform_impl::PlatformVpn;

use std::io::{BufRead, BufReader};
use std::process::{Child, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::platform::{DnsOps, PlatformOps};
use crate::util::{debug_logging_enabled, process_running};
use connection::{check_university_reachable, extract_saml_url, extract_vpn_nameservers, tunnel_connectivity_acceptable};

const GATEWAY: &str = "vpnmlk.mmu.edu.my";
const PORT: &str = "443";

#[allow(dead_code)]
pub enum Notification {
    Connected,
    Error(String),
    CampusDetected,
}

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

pub struct VpnDaemon {
    pub state: State,
    child: Option<Child>,
    last_check: Instant,
    tunnel: Arc<TunnelWatch>,
    platform: PlatformVpn,
    connect_start: Option<Instant>,
    last_health_check: Instant,
    on_campus: bool,
    pub notification_queue: Arc<Mutex<Vec<Notification>>>,
}

impl VpnDaemon {
    pub fn new() -> Self {
        let platform = PlatformVpn::new();
        println!("[vpn] Initialized platform: {}", platform.platform_name());
        Self {
            state: State::Disconnected,
            child: None,
            last_check: Instant::now(),
            tunnel: Arc::new(TunnelWatch::default()),
            platform,
            connect_start: None,
            last_health_check: Instant::now(),
            on_campus: false,
            notification_queue: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn bootstrap_cleanup(&mut self) {
        if !process_running("openfortivpn") {
            if let Err(e) = self.platform.cleanup_stale_dns() {
                eprintln!("[vpn] Startup DNS cleanup failed: {}", e);
            }
        }
    }

    pub fn start(&mut self) {
        if self.state.is_active() {
            return;
        }

        self.on_campus = check_university_reachable();
        if self.on_campus {
            println!("[vpn] University resources already accessible (campus network detected)");
            println!("[vpn] VPN connection will proceed anyway");
            if let Ok(mut queue) = self.notification_queue.lock() {
                queue.push(Notification::CampusDetected);
            }
        }

        self.state = State::Connecting;
        self.connect_start = Some(Instant::now());
        self.tunnel.up.store(false, Ordering::SeqCst);
        if let Ok(mut dns) = self.tunnel.dns_servers.lock() {
            dns.clear();
        }

        let gateway_port = format!("{}:{}", GATEWAY, PORT);
        let mut command = match self.platform.vpn_start_command(&gateway_port) {
            Ok(cmd) => cmd,
            Err(e) => {
                eprintln!("[vpn] Failed to prepare start command: {}", e);
                self.state = State::Error(format!("Setup failed: {}", e));
                return;
            }
        };

        match command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(mut child) => {
                let stdout = child.stdout.take();
                let stderr = child.stderr.take();
                let tunnel = Arc::clone(&self.tunnel);
                let debug_logs = debug_logging_enabled();

                thread::spawn(move || {
                    if let Some(stdout) = stdout {
                        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                            if debug_logs {
                                println!("[vpn] {}", line);
                            }
                            if line.contains("Authenticate at") {
                                if let Some(url) = extract_saml_url(&line) {
                                    println!("[vpn] Opening SAML URL: {}", url);
                                    if let Err(e) = open::that(&url) {
                                        eprintln!("[vpn] Failed to open URL: {}", e);
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

                let debug_logs = debug_logging_enabled();
                thread::spawn(move || {
                    if let Some(stderr) = stderr {
                        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                            if debug_logs {
                                eprintln!("[vpn] {}", line);
                            }
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

        let child_pid = self.child.as_ref().map(|c| c.id());

        if let Err(e) = self.platform.vpn_stop(child_pid) {
            eprintln!("[vpn] Stop failed: {}", e);
        }

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

        if let Err(e) = self.platform.restore_dns() {
            eprintln!("[vpn] DNS restore: {}", e);
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

        let running = process_running("openfortivpn");
        let tunnel_up = self.tunnel.up.load(Ordering::SeqCst);

        if matches!(self.state, State::Connecting) {
            if let Some(start) = self.connect_start {
                if start.elapsed() > Duration::from_secs(60) {
                    eprintln!("[vpn] Connection timeout - no tunnel after 60s");
                    let error_msg = "Connection timeout - check logs or run --cleanup".to_string();
                    self.state = State::Error(error_msg.clone());
                    self.connect_start = None;
                    if let Some(mut child) = self.child.take() {
                        let _ = child.kill();
                        let _ = child.wait();
                    }
                    // Only call vpn_stop if process is still running
                    if process_running("openfortivpn") {
                        let _ = self.platform.vpn_stop(None);
                    }
                    if let Ok(mut queue) = self.notification_queue.lock() {
                        queue.push(Notification::Error(error_msg));
                    }
                    return;
                }
            }
        }

        match (&self.state, running, tunnel_up) {
            (State::Connecting, true, true) => {
                if self.platform.needs_dns_management() {
                    let servers = self
                        .tunnel
                        .dns_servers
                        .lock()
                        .map(|g| g.clone())
                        .unwrap_or_default();
                    if !servers.is_empty() {
                        if let Err(e) = self.platform.apply_vpn_dns(&servers) {
                            eprintln!("[vpn] Failed to apply DNS: {}", e);
                        }
                    }
                }

                println!("[vpn] Tunnel reported up, verifying connectivity...");
                
                // Wait for routes and DNS to propagate after tunnel up
                // SAML auth might still be completing in background
                let tunnel_up_duration = self.connect_start
                    .map(|start| start.elapsed())
                    .unwrap_or(Duration::from_secs(0));
                
                // Wait at least 2 seconds after tunnel up before checking connectivity
                // This allows time for:
                // - SAML authentication to complete
                // - Routes to propagate
                // - DNS changes to take effect
                if tunnel_up_duration < Duration::from_secs(2) {
                    // Don't check yet, tunnel just came up
                    return;
                }
                
                // Try connectivity check with retry logic
                let mut check_succeeded = false;
                for attempt in 1..=3 {
                    if tunnel_connectivity_acceptable(self.on_campus, check_university_reachable()) {
                        check_succeeded = true;
                        break;
                    }
                    if attempt < 3 {
                        println!("[vpn] Connectivity check attempt {} failed, retrying...", attempt);
                        std::thread::sleep(Duration::from_millis(500));
                    }
                }
                
                if check_succeeded {
                    self.mark_connected();
                } else {
                    eprintln!("[vpn] Tunnel up but university resources unreachable");

                    if let Err(e) = self.platform.restore_dns() {
                        eprintln!("[vpn] Failed to rollback DNS after verification failure: {}", e);
                    } else {
                        println!("[vpn] Rolled back DNS changes due to connectivity failure");
                    }

                    let error_msg = "Tunnel up but no connectivity".to_string();
                    self.state = State::Error(error_msg.clone());
                    self.connect_start = None;
                    if let Ok(mut queue) = self.notification_queue.lock() {
                        queue.push(Notification::Error(error_msg));
                    }
                }
            }
            (State::Connecting, true, false) => {}
            (State::Connected, false, _) => {
                if let Err(e) = self.platform.restore_dns() {
                    eprintln!("[vpn] DNS restore after tunnel drop: {}", e);
                }
                self.state = State::Disconnected;
                self.tunnel.up.store(false, Ordering::SeqCst);
                self.connect_start = None;
                if let Some(mut child) = self.child.take() {
                    let _ = child.wait();
                }
            }
            (State::Connected, true, true) => {
                if self.last_health_check.elapsed() > Duration::from_secs(15) {
                    self.last_health_check = Instant::now();
                    if !check_university_reachable() {
                        eprintln!("[vpn] Health check failed - university resources unreachable");
                        let error_msg = "Connection lost - university unreachable".to_string();
                        self.state = State::Error(error_msg.clone());
                        if let Ok(mut queue) = self.notification_queue.lock() {
                            queue.push(Notification::Error(error_msg));
                        }
                    }
                }
                if self.platform.needs_dns_management() {
                    let servers = self
                        .tunnel
                        .dns_servers
                        .lock()
                        .map(|g| g.clone())
                        .unwrap_or_default();
                    if !servers.is_empty() {
                        if let Err(e) = self.platform.apply_vpn_dns(&servers) {
                            eprintln!("[vpn] Failed to apply DNS: {}", e);
                        }
                    }
                }
            }
            (State::Connecting, false, _) => {
                if let Some(ref mut child) = self.child {
                    if child.try_wait().map(|s| s.is_some()).unwrap_or(true) {
                        let error_msg = "openfortivpn process exited unexpectedly".to_string();
                        self.state = State::Error(error_msg.clone());
                        self.tunnel.up.store(false, Ordering::SeqCst);
                        self.connect_start = None;
                        if let Some(mut child) = self.child.take() {
                            let _ = child.wait();
                        }
                        if let Ok(mut queue) = self.notification_queue.lock() {
                            queue.push(Notification::Error(error_msg));
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
            (State::Error(_), _, _) => {}
            _ => {}
        }
    }

    fn mark_connected(&mut self) {
        self.state = State::Connected;
        self.connect_start = None;
        self.last_health_check = Instant::now();
        println!("[vpn] Connectivity verified - connected successfully");
        if let Ok(mut queue) = self.notification_queue.lock() {
            queue.push(Notification::Connected);
        }
    }
}

pub fn emergency_cleanup() {
    println!("Stopping openfortivpn and restoring DNS (if needed)...");
    let platform = PlatformVpn::new();
    if let Err(e) = platform.emergency_cleanup() {
        eprintln!("Emergency cleanup failed: {}", e);
    }
}
