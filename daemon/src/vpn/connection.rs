use std::process::{Command, Stdio};

pub fn extract_saml_url(line: &str) -> Option<String> {
    let start = line.find('\'')? + 1;
    let end = line[start..].find('\'')? + start;
    let candidate = &line[start..end];
    if candidate.starts_with("https://") || candidate.starts_with("http://") {
        Some(candidate.to_string())
    } else {
        None
    }
}

pub fn extract_vpn_nameservers(line: &str) -> Option<Vec<String>> {
    let marker = "ns [";
    let start = line.find(marker)? + marker.len();
    let end = line[start..].find(']')? + start;
    let body = line[start..end].trim();
    if body.is_empty() {
        return None;
    }
    let servers: Vec<String> = body
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if servers.is_empty() {
        None
    } else {
        Some(servers)
    }
}

pub fn tunnel_connectivity_acceptable(on_campus: bool, reachable_now: bool) -> bool {
    on_campus || reachable_now
}

pub fn check_university_reachable() -> bool {
    // Use ping instead of TCP connection - more reliable for early tunnel checks
    // and matches the pre-refactor behavior that worked correctly
    Command::new("ping")
        .args(["-c", "1", "-W", "2", "erep.mmu.edu.my"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_saml_url_from_openfortivpn_output() {
        let line = "[vpn] Authenticate at 'https://vpn.example.test/saml?token=abc'";
        assert_eq!(
            extract_saml_url(line),
            Some("https://vpn.example.test/saml?token=abc".to_string())
        );
    }

    #[test]
    fn returns_none_when_saml_url_is_missing() {
        assert_eq!(extract_saml_url("[vpn] Waiting for authentication"), None);
    }

    #[test]
    fn ignores_non_url_saml_browser_tokens() {
        assert_eq!(
            extract_saml_url("[vpn] Authenticate at 'use_default'"),
            None
        );
    }

    #[test]
    fn extracts_vpn_nameservers_from_openfortivpn_output() {
        let line = "INFO:   Got addresses: [10.9.12.1], ns [10.242.3.201, 10.242.3.200]";
        assert_eq!(
            extract_vpn_nameservers(line),
            Some(vec!["10.242.3.201".to_string(), "10.242.3.200".to_string()])
        );
    }

    #[test]
    fn returns_none_when_nameservers_missing() {
        assert_eq!(extract_vpn_nameservers("INFO: Tunnel is up"), None);
    }

    #[test]
    fn campus_detected_allows_tunnel_without_rechecking_reachability() {
        assert!(tunnel_connectivity_acceptable(true, false));
    }
}
