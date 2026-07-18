use sysinfo::{ProcessesToUpdate, System};

pub fn debug_logging_enabled() -> bool {
    std::env::var("MMUVPN_DEBUG").is_ok_and(|value| value == "1")
}

pub fn process_running(name: &str) -> bool {
    let mut system = System::new();
    system.refresh_processes(ProcessesToUpdate::All, true);
    system
        .processes()
        .values()
        .any(|proc| {
            // Handle Linux kernel truncation at 15 chars (e.g., "openfortivpn" -> "openfortiv")
            let proc_name = proc.name().to_str().unwrap_or("");
            proc_name == name || proc_name.starts_with(name) || name.starts_with(proc_name)
        })
}
