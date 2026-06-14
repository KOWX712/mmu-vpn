#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

echo "Building mmuvpn..."
cargo build --release --manifest-path "$SCRIPT_DIR/daemon/Cargo.toml"

echo "Creating dist/..."
rm -rf "$SCRIPT_DIR/dist"
DIST="$SCRIPT_DIR/dist/usr"
mkdir -p "$DIST/bin" "$DIST/lib/systemd/user" "$DIST/share/applications" "$DIST/share/polkit-1/actions" "$DIST/share/polkit-1/rules.d"

cp "$SCRIPT_DIR/daemon/target/release/mmuvpn" "$DIST/bin/mmuvpn"
chmod +x "$DIST/bin/mmuvpn"

install -Dm644 "$SCRIPT_DIR/daemon/mmuvpn.service" "$DIST/lib/systemd/user/mmuvpn.service"
install -Dm644 "$SCRIPT_DIR/daemon/mmuvpn.desktop" "$DIST/share/applications/mmuvpn.desktop"
install -Dm644 "$SCRIPT_DIR/daemon/polkit/cc.kowx712.fortivpn.policy" "$DIST/share/polkit-1/actions/cc.kowx712.fortivpn.policy"
install -Dm644 "$SCRIPT_DIR/daemon/polkit/50-openfortivpn.rules" "$DIST/share/polkit-1/rules.d/50-openfortivpn.rules"

echo "dist/ created:"
find "$SCRIPT_DIR/dist" -type f | sort
