fn main() {
    // Tell Cargo to re-run this build script if template files change
    println!("cargo:rerun-if-changed=templates/askpass.sh");
    println!("cargo:rerun-if-changed=templates/systemd.service");
    println!("cargo:rerun-if-changed=templates/launchd.plist");
}
