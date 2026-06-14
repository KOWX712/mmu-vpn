use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const GATEWAY: &str = "vpnmlk.mmu.edu.my";
const PORT: &str = "443";

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum State {
    Disconnected,
    Connecting,
    Connected,
}

impl State {
    pub fn label(self) -> &'static str {
        match self {
            State::Disconnected => "Disconnected",
            State::Connecting => "Connecting",
            State::Connected => "Connected",
        }
    }
}

pub struct VpnDaemon {
    pub state: State,
    child: Option<Child>,
    last_check: Instant,
}

impl VpnDaemon {
    pub fn new() -> Self {
        Self {
            state: State::Disconnected,
            child: None,
            last_check: Instant::now(),
        }
    }

    pub fn start(&mut self) {
        if self.state != State::Disconnected {
            return;
        }
        self.state = State::Connecting;

        let args = format!("{}:{}", GATEWAY, PORT);
        match Command::new("pkexec")
            .args(["openfortivpn", &args, "--saml-login"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(mut child) => {
                let stdout = child.stdout.take();
                let stderr = child.stderr.take();

                thread::spawn(move || {
                    if let Some(stdout) = stdout {
                        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                            println!("[vpn] {}", line);
                            if line.contains("Authenticate at") {
                                if let Some(url) = extract_saml_url(&line) {
                                    println!("[vpn] Opening SAML URL: {}", url);
                                    let _ = Command::new("xdg-open").arg(&url).spawn();
                                }
                            }
                            if line.contains("Tunnel is up") {
                                println!("[vpn] Connected!");
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
        // Send SIGINT to openfortivpn via pkexec for clean shutdown
        if let Ok(output) = Command::new("pgrep").arg("openfortivpn").output() {
            let pids = String::from_utf8_lossy(&output.stdout);
            for pid in pids.lines() {
                let pid = pid.trim();
                if !pid.is_empty() {
                    let _ = Command::new("pkexec")
                        .args(["kill", "-INT", pid])
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .spawn();
                }
            }
        }
        // Kill pkexec itself (we own it)
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
        }
        self.state = State::Disconnected;
    }

    pub fn check_alive(&mut self) {
        if self.last_check.elapsed() < Duration::from_millis(300) {
            return;
        }
        self.last_check = Instant::now();

        let running = Command::new("pgrep")
            .arg("openfortivpn")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        match (self.state, running) {
            (State::Connecting, true) => self.state = State::Connected,
            (State::Connected, false) => {
                self.state = State::Disconnected;
                self.child = None;
            }
            (State::Connecting, false) => {
                // Give pkexec a moment to spawn openfortivpn
                if self.child.is_some() {
                    // Check if pkexec itself is still alive
                    if let Some(ref mut child) = self.child {
                        if child.try_wait().map(|s| s.is_some()).unwrap_or(true) {
                            self.state = State::Disconnected;
                            self.child = None;
                        }
                    }
                } else {
                    self.state = State::Disconnected;
                }
            }
            (State::Disconnected, false) => {
                self.child = None;
            }
            _ => {}
        }
    }
}

fn extract_saml_url(line: &str) -> Option<String> {
    let start = line.find('\'')? + 1;
    let end = line[start..].find('\'')? + start;
    Some(line[start..end].to_string())
}
