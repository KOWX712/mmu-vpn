mod tray;
mod vpn;

use std::fs;
use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const SOCKET_PATH: &str = "/tmp/mmuvpn.sock";

struct SingleInstance {
    _listener: UnixListener,
}

impl SingleInstance {
    fn acquire() -> Option<Self> {
        let path = PathBuf::from(SOCKET_PATH);
        if UnixStream::connect(&path).is_ok() {
            return None;
        }
        let _ = fs::remove_file(&path);
        let listener = UnixListener::bind(&path).ok()?;
        listener.set_nonblocking(true).ok();
        Some(SingleInstance { _listener: listener })
    }

    fn accept_pending(&self) -> Option<String> {
        let (mut stream, _) = self._listener.accept().ok()?;
        let mut buf = [0u8; 64];
        let n = stream.read(&mut buf).ok()?;
        if n == 0 {
            return None;
        }
        Some(String::from_utf8_lossy(&buf[..n]).trim().to_string())
    }
}

impl Drop for SingleInstance {
    fn drop(&mut self) {
        let _ = fs::remove_file(SOCKET_PATH);
    }
}

fn send_command(cmd: &str) -> bool {
    let path = PathBuf::from(SOCKET_PATH);
    if let Ok(mut stream) = UnixStream::connect(&path) {
        stream.write_all(cmd.as_bytes()).is_ok()
    } else {
        false
    }
}

fn usage() {
    eprintln!("Usage: mmuvpn [OPTIONS]");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  (no args)    Start tray daemon");
    eprintln!("  --start      Start VPN (and tray if not running)");
    eprintln!("  --stop       Stop VPN");
    eprintln!("  --quit       Kill all and exit daemon");
    eprintln!("  --help       Show this help");
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match args.first().map(|s| s.as_str()) {
        Some("--help") | Some("-h") => {
            usage();
        }
        Some("--stop") => {
            if send_command("stop") {
                println!("VPN stopped");
            } else {
                let mut daemon = vpn::VpnDaemon::new();
                daemon.stop();
                println!("VPN stopped");
            }
        }
        Some("--quit") => {
            if send_command("quit") {
                println!("Daemon quit");
            } else {
                println!("No daemon running");
            }
        }
        Some("--start") => {
            if send_command("start") {
                println!("VPN start signal sent");
            } else {
                let _inst = SingleInstance::acquire().unwrap_or_else(|| {
                    eprintln!("Another mmuvpn instance is already running");
                    std::process::exit(1);
                });
                let daemon = Arc::new(Mutex::new(vpn::VpnDaemon::new()));
                daemon.lock().unwrap().start();
                tray::run(daemon, false, _inst);
            }
        }
        None => {
            let _inst = SingleInstance::acquire().unwrap_or_else(|| {
                eprintln!("Another mmuvpn instance is already running");
                std::process::exit(1);
            });
            let daemon = Arc::new(Mutex::new(vpn::VpnDaemon::new()));
            tray::run(daemon, false, _inst);
        }
        Some(other) => {
            eprintln!("Unknown option: {}", other);
            usage();
            std::process::exit(1);
        }
    }
}
