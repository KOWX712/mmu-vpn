# MMU VPN

OpenFortiVPN tray wrapper for **Multimedia University**. Replaces the proprietary FortiClient VPN with a lightweight system tray daemon.

## Features

- System tray icon with live VPN status (disconnected / connecting / connected)
- SAML SSO authentication — opens browser automatically
- Systemd user service for auto-start on login

## Requirements

- `openfortivpn`
- `polkit`
- User in `wheel` group

## Installation

### Arch Linux (AUR)

```bash
yay -S mmu-vpn-bin
```

### Debian/Ubuntu

Download `.deb` from [GitHub Releases](https://github.com/KOWX712/mmu-vpn/releases).

```bash
sudo dpkg -i mmu-vpn_*.deb
sudo apt-get install -f
```

### Fedora/RHEL

Download `.rpm` from [GitHub Releases](https://github.com/KOWX712/mmu-vpn/releases).

```bash
sudo dnf install mmu-vpn-*.rpm
```

### Build from source

```bash
# Arch
sudo pacman -S rust openfortivpn polkit

cargo build --release
sudo cp target/release/mmuvpn /usr/bin/
```

## Usage

### Systemd (auto-start on login)

```bash
systemctl --user enable --now mmuvpn.service
```
