mod tray;
mod vpn;

use std::sync::{Arc, Mutex};

fn usage() {
    eprintln!("Usage: mmuvpn [OPTIONS]");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  (no args)    Start tray daemon");
    eprintln!("  --start      Start VPN and show tray");
    eprintln!("  --stop       Stop VPN and exit");
    eprintln!("  --quit       Kill all and exit");
    eprintln!("  --help       Show this help");
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match args.first().map(|s| s.as_str()) {
        Some("--help") | Some("-h") => {
            usage();
        }
        Some("--stop") => {
            let mut daemon = vpn::VpnDaemon::new();
            daemon.stop();
            println!("VPN stopped");
        }
        Some("--quit") => {
            let _ = std::process::Command::new("pkexec")
                .args(["killall", "-9", "openfortivpn"])
                .spawn();
            println!("All VPN processes killed");
        }
        Some("--start") => {
            let daemon = Arc::new(Mutex::new(vpn::VpnDaemon::new()));
            daemon.lock().unwrap().start();
            tray::run(daemon, false);
        }
        None => {
            let daemon = Arc::new(Mutex::new(vpn::VpnDaemon::new()));
            tray::run(daemon, false);
        }
        Some(other) => {
            eprintln!("Unknown option: {}", other);
            usage();
            std::process::exit(1);
        }
    }
}
