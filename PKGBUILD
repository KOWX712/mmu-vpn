# Maintainer: KOWX712 <KOWX712@leecc0503@gmail.com>
pkgname=mmu-vpn-bin
pkgver=0.1.0
pkgrel=1
pkgdesc="OpenFortiVPN tray wrapper for Multimedia University (MMU)"
arch=('x86_64')
url="https://github.com/KOWX712/mmu-vpn"
license=('GPL-3.0')
depends=('openfortivpn' 'polkit' 'xdotool')
provides=('mmu-vpn')
conflicts=('mmu-vpn')
source=("$pkgname-$pkgver.tar.gz")
sha256sums=('SKIP')

package() {
    cp -r "$srcdir/dist/"* "$pkgdir/"
}
