# MMU VPN

OpenFortiVPN tray wrapper for **Multimedia University**. Replaces the proprietary FortiClient VPN with a lightweight system tray daemon.

## Features

- System tray icon with live VPN status (disconnected / connecting / connected)
- SAML SSO authentication — opens browser automatically
- Linux systemd user service for auto-start on login
- macOS menu bar support with LaunchAgent auto-start option

## Requirements

- [openfortivpn](https://github.com/adrienverge/openfortivpn)
- Linux: [polkit](https://github.com/polkit-org/polkit), user in `wheel` group
- macOS: [Homebrew](https://brew.sh/) and `brew install openfortivpn`

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

Install the prerequisites.

```bash
brew install openfortivpn
```

Install the latest release (prebuilt binary).

```bash
curl -LSs https://raw.githubusercontent.com/KOWX712/mmu-vpn/master/install.sh | bash
```

The installer places `mmuvpn` in `/opt/homebrew/bin` and writes a LaunchAgent to
`~/Library/LaunchAgents/cc.kowx712.mmuvpn.plist`.

On macOS, the app applies VPN DNS with `networksetup` after the tunnel comes up
and restores previous DNS on disconnect. Previous DNS is saved at
`/tmp/mmuvpn-dns-backup` so `mmuvpn --cleanup` can recover after a crash.

If DNS or the VPN is left stuck after a crash, run:

```bash
mmuvpn --cleanup
```

That stops openfortivpn (if any) and restores DNS even when the tray is not running.

Enable auto-start on login:

```bash
launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/cc.kowx712.mmuvpn.plist
```

Disable auto-start:

```bash
launchctl bootout gui/$(id -u) ~/Library/LaunchAgents/cc.kowx712.mmuvpn.plist
```

## Usage

### Systemd (auto-start on login)

```bash
systemctl --user enable --now mmuvpn.service
```
