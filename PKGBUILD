# Maintainer: KOWX712 <KOWX712@leecc0503@gmail.com>
pkgname=mmu-vpn
pkgver=0.1.0
pkgrel=2
pkgdesc="OpenFortiVPN tray wrapper for Multimedia University"
arch=('x86_64')
url="https://github.com/KOWX712/mmu-vpn"
license=('GPL-3.0')
depends=('openfortivpn' 'polkit' 'xdotool')
source=("$url/releases/download/v$pkgver/$pkgname-bin-$pkgver.tar.gz")
sha256sums=('8e507e5f9b9232cf1dff4eef2ffd3c7f3725b35f71087281b8a4bed3c7b201c2')

package() {
    cp -r "$srcdir/dist/"* "$pkgdir/"
}
