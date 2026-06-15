# MMU VPN

OpenFortiVPN tray wrapper for **Multimedia University**. Replaces the proprietary FortiClient VPN with a lightweight system tray daemon.

## Features

- System tray icon with live VPN status (disconnected / connecting / connected)
- SAML SSO authentication — opens browser automatically
- Systemd user service for auto-start on login

## Requirements

- [openfortivpn](https://github.com/adrienverge/openfortivpn)
- [polkit](https://github.com/polkit-org/polkit)
- User in `wheel` group

## Installation

### Quick Install or Update

Universal installation method.

```bash
# Install or update latest release
curl -LSs https://raw.githubusercontent.com/KOWX712/mmu-vpn/master/install.sh | bash

# Install a specific version
curl -LSs https://raw.githubusercontent.com/KOWX712/mmu-vpn/master/install.sh | bash -s -- v0.1.0
```

### Arch Linux (AUR)

```bash
yay -S mmu-vpn
# or
paru -S mmu-vpn
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

## Usage

### Systemd (auto-start on login)

```bash
systemctl --user enable --now mmuvpn.service
```
