pub fn openfortivpn_bin() -> String {
    which::which("openfortivpn")
        .ok()
        .and_then(|p| p.to_str().map(|s| s.to_string()))
        .or_else(|| {
            [
                "/opt/homebrew/bin/openfortivpn",
                "/usr/local/bin/openfortivpn",
                "/usr/bin/openfortivpn",
            ]
            .into_iter()
            .find(|p| std::path::Path::new(p).exists())
            .map(str::to_string)
        })
        .unwrap_or_else(|| "openfortivpn".to_string())
}
