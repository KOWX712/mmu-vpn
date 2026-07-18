#!/usr/bin/env bash

set -euo pipefail

REPO="KOWX712/mmu-vpn"
BINARY="mmuvpn"
PKG_NAME="mmu-vpn"

usage() {
    cat <<EOF
Usage: $(basename "$0") [OPTIONS] [TAG]

Install or update ${PKG_NAME}.

Options:
  -h, --help    Show this help message

Arguments:
  TAG           Specific release tag (e.g. v0.1.0). Defaults to latest release.

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
    local deps=(curl tar)
    for cmd in "${deps[@]}"; do
        command -v "$cmd" &>/dev/null || die "Required command '${cmd}' not found. Please install it first."
    done

    command -v brew &>/dev/null || die "Homebrew is required on macOS. Install it from https://brew.sh, then rerun this script."
}

detect_distro() {
    if [ "$(uname -s)" = "Darwin" ]; then
        echo "darwin"
        return
    fi

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

clear_macos_quarantine() {
    local install_path="$1"

    if command -v xattr &>/dev/null; then
        xattr -d com.apple.quarantine "$install_path" 2>/dev/null || true
    fi
}

install_macos() {
    local tag="$1"
    local app_name="MMU VPN.app"
    local install_path="/Applications/${app_name}"

    check_macos_deps
    command -v openfortivpn &>/dev/null || brew install openfortivpn || die "Failed to install openfortivpn. Install it manually with: brew install openfortivpn"

    local filename="mmu-vpn-macos-universal.tar.gz"
    local url="https://github.com/${REPO}/releases/download/${tag}/${filename}"
    echo "Downloading $url"
    local tmpdir
    tmpdir=$(mktemp -d)
    trap "rm -rf '$tmpdir'" EXIT
    curl -fSL -o "${tmpdir}/${filename}" "$url" || die "Prebuilt binary not available at ${tag}"
    tar -xzf "${tmpdir}/${filename}" -C "$tmpdir"

    # Remove old installation if exists
    if [ -d "$install_path" ]; then
        echo "Removing old installation..."
        rm -rf "$install_path"
    fi

    # Install app bundle
    echo "Installing to ${install_path}..."
    cp -r "${tmpdir}/${app_name}" "$install_path"

    # Clear quarantine
    if command -v xattr &>/dev/null; then
        if ! xattr -dr com.apple.quarantine "$install_path" 2>/dev/null; then
            echo "Warning: Failed to remove quarantine attribute. You may need to right-click and select 'Open' on first launch."
        fi
    fi

    echo "Installed $PKG_NAME $tag"
    echo ""
    echo "To run: open '${install_path}'"
    echo "Or: double-click 'MMU VPN' in Applications folder"
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
        darwin) install_macos "$tag" ;;
        debian) install_debian "$tag" ;;
        rhel)   install_rhel "$tag" ;;
        arch)   install_arch "$tag" ;;
    esac
}

main "$@"
