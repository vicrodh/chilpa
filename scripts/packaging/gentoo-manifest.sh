#!/bin/bash
# Runs inside a gentoo/stage3 container with the portage tree mounted.
# Regenerates the thin Manifest of media-sound/chilpa-bin in /overlay.
#   /overlay    the overlay checkout (read-write)
#   /distfiles  optional directory holding distfiles that are already downloaded
set -eu
export FEATURES="-sandbox -usersandbox -ipc-sandbox -network-sandbox -pid-sandbox"
mkdir -p /etc/portage/repos.conf /var/cache/distfiles
cp /usr/share/portage/config/repos.conf /etc/portage/repos.conf/gentoo.conf
cp /distfiles/* /var/cache/distfiles/ 2>/dev/null || true
printf '[chilpa]\nlocation = /overlay\n' > /etc/portage/repos.conf/chilpa.conf
pkg=/overlay/media-sound/chilpa-bin
latest="$(ls "$pkg"/chilpa-bin-*.ebuild | sort -V | tail -1)"
ebuild "$latest" manifest
# The container runs as root; hand the files back to the caller.
if [ -n "${HOST_UID:-}" ]; then chown -R "${HOST_UID}:${HOST_GID:-$HOST_UID}" /overlay; fi
