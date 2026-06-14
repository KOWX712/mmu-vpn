# Maintainer: KOWX712 <KOWX712@leecc0503@gmail.com>
pkgname=mmu-vpn
pkgver=0.1.0
pkgrel=3
pkgdesc="OpenFortiVPN tray wrapper for Multimedia University"
arch=('x86_64')
url="https://github.com/KOWX712/mmu-vpn"
license=('GPL-3.0')
depends=('openfortivpn' 'polkit' 'xdotool')
source=("$url/releases/download/v$pkgver/$pkgname-bin-$pkgver.tar.gz")
sha256sums=('1c3158b9d3ea87b8b321a01c39db8a365a513506475d8f3b200332c9504ba212')

package() {
    cp -r "$srcdir/dist/"* "$pkgdir/"
}
