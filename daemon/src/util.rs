pub fn debug_logging_enabled() -> bool {
    std::env::var("MMUVPN_DEBUG").is_ok_and(|value| value == "1")
}
