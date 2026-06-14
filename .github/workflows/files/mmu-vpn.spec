Name:           mmu-vpn
Version:        %VERSION%
Release:        1
Summary:        OpenFortiVPN tray wrapper for MMU
License:        GPL-3.0
Requires:       openfortivpn polkit libxdo

%description
OpenFortiVPN tray wrapper for Multimedia University (MMU)

%install
mkdir -p %{buildroot}/usr/bin
mkdir -p %{buildroot}/usr/lib/systemd/user
mkdir -p %{buildroot}/usr/share/applications
mkdir -p %{buildroot}/usr/share/polkit-1/actions
mkdir -p %{buildroot}/usr/share/polkit-1/rules.d
install -Dm755 %{_sourcedir}/daemon/target/release/mmuvpn %{buildroot}/usr/bin/mmuvpn
install -Dm644 %{_sourcedir}/daemon/mmuvpn.service %{buildroot}/usr/lib/systemd/user/mmuvpn.service
install -Dm644 %{_sourcedir}/daemon/mmuvpn.desktop %{buildroot}/usr/share/applications/mmuvpn.desktop
install -Dm644 %{_sourcedir}/daemon/polkit/cc.kowx712.fortivpn.policy %{buildroot}/usr/share/polkit-1/actions/cc.kowx712.fortivpn.policy
install -Dm644 %{_sourcedir}/daemon/polkit/50-openfortivpn.rules %{buildroot}/usr/share/polkit-1/rules.d/50-openfortivpn.rules

%files
/usr/bin/mmuvpn
/usr/lib/systemd/user/mmuvpn.service
/usr/share/applications/mmuvpn.desktop
/usr/share/polkit-1/actions/cc.kowx712.fortivpn.policy
/usr/share/polkit-1/rules.d/50-openfortivpn.rules
