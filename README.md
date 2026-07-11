# MMU VPN

OpenFortiVPN tray wrapper for **Multimedia University**. Replaces the proprietary FortiClient VPN with a lightweight system tray daemon.

## Features

- System tray icon with live VPN status (disconnected / connecting / connected)
- SAML SSO authentication — opens browser automatically
- Linux and macOS login service management from the CLI

## Requirements

- [openfortivpn](https://github.com/adrienverge/openfortivpn)
- Linux: [polkit](https://github.com/polkit-org/polkit), user in `wheel` group
- macOS: [Homebrew](https://brew.sh/)

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

### macOS

Use the universal installation method.

On macOS, the app applies VPN DNS with `networksetup` after the tunnel comes up and restores previous DNS on disconnect. Previous DNS is saved at `/tmp/mmuvpn-dns-backup` so `mmuvpn --cleanup` can recover after a crash.

If DNS or the VPN is left stuck after a crash, run:

```bash
mmuvpn --cleanup
```

That stops openfortivpn (if any) and restores DNS even when the tray is not running.

## Usage

### Tray App

- Launch and all set, click the tray icon to control the connection status.

### CLI

| Command | Description |
| --- | --- |
| `mmuvpn` | Start the tray daemon |
| `mmuvpn --start` | Start VPN (and tray if not running) |
| `mmuvpn --stop` | Stop VPN and restore DNS on macOS |
| `mmuvpn --quit` | Stop VPN, restore DNS, and exit daemon |
| `mmuvpn --cleanup` | Emergency cleanup for leftover VPN/DNS state |
| `mmuvpn --service status` | Show the login service status |
| `mmuvpn --service enable` | Install and start the login service |
| `mmuvpn --service disable` | Stop and remove the login service |
