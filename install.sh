#!/usr/bin/env bash

set -euo pipefail

REPO="KOWX712/mmu-vpn"
BINARY="mmuvpn"
PKG_NAME="mmu-vpn"
INSTALL_DIR="/usr/bin"

usage() {
    cat <<EOF
Usage: $(basename "$0") [OPTIONS] [TAG]

Install or update ${PKG_NAME}.

Options:
  -h, --help    Show this help message

Arguments:
  TAG           Specific release tag (e.g. v0.1.0). Linux defaults to latest release.
                macOS defaults to the current checkout or default branch.

Examples:
  curl -LSs https://raw.githubusercontent.com/${REPO}/master/install.sh | bash
  curl -LSs https://raw.githubusercontent.com/${REPO}/master/install.sh | bash -s -- v0.1.0
EOF
}

die() { echo "Error: $1" >&2; exit 1; }

check_deps() {
    local deps=(curl tar git)
    for cmd in "${deps[@]}"; do
        command -v "$cmd" &>/dev/null || die "Required command '${cmd}' not found. Please install it first."
    done
}

check_macos_deps() {
    local deps=(brew cargo git)
    for cmd in "${deps[@]}"; do
        command -v "$cmd" &>/dev/null || die "Required command '${cmd}' not found. Please install it first."
    done

    if ! command -v openfortivpn &>/dev/null; then
        die "openfortivpn not found. Install it with: brew install openfortivpn"
    fi

    require_modern_cargo
}

require_modern_cargo() {
    local version major minor
    version=$(cargo --version | awk '{print $2}')
    major=${version%%.*}
    minor=${version#*.}
    minor=${minor%%.*}

    if [ "$major" -lt 1 ] || { [ "$major" -eq 1 ] && [ "$minor" -lt 85 ]; }; then
        die "Cargo ${version} is too old. Install Rust/Cargo 1.85+ with rustup or update Homebrew Rust."
    fi
}

detect_distro() {
    if [ -f /etc/os-release ]; then
        . /etc/os-release
        case "$ID" in
            debian|ubuntu|linuxmint|pop|raspbian|kali|devuan|zorin|elementary|pureos|deepin|uos)
                echo "debian" ;;
            fedora|rhel|centos|rocky|alma|ol|nobara|ultramarine)
                echo "rhel" ;;
            arch|manjaro|endeavouros|garuda|blendos|cachyos)
                echo "arch" ;;
            *)
                # Fallback: check for ID_LIKE
                case "${ID_LIKE:-}" in
                    *debian*) echo "debian" ;;
                    *rhel*|*fedora*) echo "rhel" ;;
                    *arch*) echo "arch" ;;
                    *) die "Unsupported distro: ${ID:-unknown} (${ID_LIKE:-no ID_LIKE})" ;;
                esac ;;
        esac
    else
        die "Cannot detect distro: /etc/os-release not found"
    fi
}

get_latest_tag() {
    curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name"' | head -1 | cut -d '"' -f4
}

get_arch() {
    local arch
    arch=$(uname -m)
    case "$arch" in
        x86_64|amd64) echo "amd64" ;;
        # aarch64|arm64) echo "arm64" ;;
        # armv7l|armhf) echo "armhf" ;;
        *) die "Unsupported architecture: ${arch}" ;;
    esac
}

install_debian() {
    local tag="$1"
    local arch
    arch=$(get_arch)
    local filename="${PKG_NAME}_${tag#v}_amd64.deb"
    local url="https://github.com/${REPO}/releases/download/${tag}/${filename}"

    echo "Downloading $url"
    local tmpdir
    tmpdir=$(mktemp -d)
    trap "rm -rf '$tmpdir'" EXIT

    curl -fSL -o "${tmpdir}/${filename}" "$url"

    sudo dpkg -i "${tmpdir}/${filename}" || sudo apt-get install -f -y

    echo "Installed $PKG_NAME $tag"
}

install_rhel() {
    local tag="$1"
    local filename="${PKG_NAME}-${tag#v}-1.x86_64.rpm"
    local url="https://github.com/${REPO}/releases/download/${tag}/${filename}"

    echo "Downloading $url"
    local tmpdir
    tmpdir=$(mktemp -d)
    trap "rm -rf '$tmpdir'" EXIT

    curl -fSL -o "${tmpdir}/${filename}" "$url"

    if command -v dnf &>/dev/null; then
        sudo dnf install -y "${tmpdir}/${filename}"
    elif command -v yum &>/dev/null; then
        sudo yum install -y "${tmpdir}/${filename}"
    else
        sudo rpm -i "${tmpdir}/${filename}"
    fi

    echo "Installed $PKG_NAME $tag"
}

install_arch() {
    if command -v yay &>/dev/null; then
        yay -S --noconfirm "$PKG_NAME"
    elif command -v paru &>/dev/null; then
        paru -S --noconfirm "$PKG_NAME"
    else
        echo "No AUR helper found. Building from source via makepkg."
        command -v makepkg &>/dev/null || die "makepkg not found. Install base-devel first: sudo pacman -S base-devel"
        command -v cargo &>/dev/null || die "cargo not found. Install rust first: sudo pacman -S rust"

        local tag="$1"
        local build_dir
        build_dir=$(mktemp -d)

        echo "Cloning from GitHub..."
        local clone_args=(--depth 1)
        [ -n "$tag" ] && clone_args+=(-b "$tag")
        git clone "${clone_args[@]}" "https://github.com/${REPO}.git" "${build_dir}/${PKG_NAME}"

        sed -i "s|pkgver=.*|pkgver=$tag|g" "${build_dir}/${PKG_NAME}/PKGBUILD"

        makepkg -si --noconfirm --needed -D "${build_dir}/${PKG_NAME}"

        rm -rf "$build_dir"

        echo "Installed $PKG_NAME $tag"
    fi
}

install_file() {
    local src="$1"
    local dest="$2"
    local mode="$3"
    local dir
    dir=$(dirname "$dest")

    if [ -w "$dir" ]; then
        install -m "$mode" "$src" "$dest"
    else
        sudo install -m "$mode" "$src" "$dest"
    fi
}

install_launch_agent() {
    local binary_path="$1"
    local brew_prefix="$2"
    local agents_dir="$HOME/Library/LaunchAgents"
    local plist_path="${agents_dir}/cc.kowx712.mmuvpn.plist"

    mkdir -p "$agents_dir"
    cat > "$plist_path" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>cc.kowx712.mmuvpn</string>
  <key>ProgramArguments</key>
  <array>
    <string>${binary_path}</string>
  </array>
  <key>EnvironmentVariables</key>
  <dict>
    <key>PATH</key>
    <string>${brew_prefix}/bin:${brew_prefix}/sbin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin</string>
  </dict>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <false/>
  <key>StandardOutPath</key>
  <string>/tmp/mmuvpn.log</string>
  <key>StandardErrorPath</key>
  <string>/tmp/mmuvpn.err</string>
</dict>
</plist>
PLIST

    echo "LaunchAgent written to ${plist_path}"
    echo "Enable login start with:"
    echo "  launchctl bootstrap gui/$(id -u) ${plist_path}"
    echo "Disable login start with:"
    echo "  launchctl bootout gui/$(id -u) ${plist_path}"
}

install_macos() {
    local tag="$1"

    check_macos_deps

    local brew_prefix install_dir install_path src_dir build_dir
    brew_prefix=$(brew --prefix)
    install_dir="${brew_prefix}/bin"
    install_path="${install_dir}/${BINARY}"

    if [ -f "daemon/Cargo.toml" ]; then
        src_dir=$(pwd)
        build_dir=""
        echo "Building from current checkout."
    else
        build_dir=$(mktemp -d)
        src_dir="${build_dir}/${PKG_NAME}"
        local clone_args=(--depth 1)
        [ -n "$tag" ] && clone_args+=(-b "$tag")

        echo "Cloning from GitHub..."
        git clone "${clone_args[@]}" "https://github.com/${REPO}.git" "$src_dir"
    fi

    cargo build --locked --release --manifest-path "${src_dir}/daemon/Cargo.toml"
    install_file "${src_dir}/daemon/target/release/${BINARY}" "$install_path" 755
    install_launch_agent "$install_path" "$brew_prefix"

    [ -n "$build_dir" ] && rm -rf "$build_dir"

    echo "Installed $PKG_NAME to ${install_path}"
}

main() {
    local tag=""

    while [ $# -gt 0 ]; do
        case "$1" in
            -h|--help) usage; exit 0 ;;
            -*) die "Unknown option: $1" ;;
            *) tag="$1" ;;
        esac
        shift
    done

    if [ "$(uname -s)" = "Darwin" ]; then
        install_macos "$tag"
        exit 0
    fi

    check_deps

    if [ -z "$tag" ]; then
        echo "Fetching latest release..."
        tag=$(get_latest_tag) || die "Failed to fetch latest release from GitHub"
    fi

    echo "Target: $PKG_NAME $tag"

    local distro
    distro=$(detect_distro)
    echo "Detected distro family: $distro"

    case "$distro" in
        debian) install_debian "$tag" ;;
        rhel)   install_rhel "$tag" ;;
        arch)   install_arch "$tag" ;;
    esac
}

main "$@"
