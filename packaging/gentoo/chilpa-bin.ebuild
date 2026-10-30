EAPI=8

inherit desktop xdg

DESCRIPTION="Native music player with local libraries and extensible sources"
HOMEPAGE="https://chilpa.app https://github.com/vicrodh/chilpa"
SRC_URI="
	amd64? ( https://github.com/vicrodh/chilpa/releases/download/v${PV}/chilpa-${PV}-linux-x86_64.tar.gz )
	arm64? ( https://github.com/vicrodh/chilpa/releases/download/v${PV}/chilpa-${PV}-linux-aarch64.tar.gz )
"
S="${WORKDIR}/chilpa-${PV}"

LICENSE="MIT Apache-2.0 BSD-2 ISC OFL-1.1"
SLOT="0"
KEYWORDS="-* ~amd64 ~arm64"
RESTRICT="strip"

RDEPEND="
	>=sys-libs/glibc-2.39
	dev-db/sqlite:3
	dev-libs/openssl:0/3
	dev-qt/qtbase:6[dbus,gui,network,opengl,sql,sqlite,ssl,wayland]
	dev-qt/qtdeclarative:6[opengl,wayland]
	dev-qt/qtsvg:6
	dev-qt/qtwayland:6
	media-libs/alsa-lib
	media-libs/libpulse
	media-video/pipewire
	sys-apps/bubblewrap
	sys-apps/dbus
"

QA_PREBUILT="usr/bin/chilpa-qt"

src_install() {
	dobin chilpa-qt
	domenu app.chilpa.Chilpa.desktop
	doicon -s 512 icons/hicolor/512x512/apps/app.chilpa.Chilpa.png
	dodoc -r licenses
}
