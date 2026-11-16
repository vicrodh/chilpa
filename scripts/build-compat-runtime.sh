#!/usr/bin/env bash
# Builds the compatibility worker's CPAN distributions into DEST/lib/perl5,
# the `libraries` folder scripts/package-compat.sh copies beside the binary.
#
#   build-compat-runtime.sh PERL DEST SOURCES DIST...
#
# Each DIST is a folder under SOURCES holding one unpacked distribution. They
# are built in the order given, each seeing the ones before it, so a module
# needed to configure another comes first. Tests, manual pages and install
# records are left out; the result depends only on the sources and on PERL.
set -euo pipefail

[[ $# -ge 4 ]] || { printf 'usage: %s PERL DEST SOURCES DIST...\n' "$0" >&2; exit 2; }
perl="$1"
dest="$2"
sources="$3"
shift 3
[[ -x "$perl" ]] || { printf 'not executable: %s\n' "$perl" >&2; exit 2; }
mkdir -p "$dest/lib/perl5"
dest="$(cd "$dest" && pwd)"

# Only what is built here and Perl's own modules are visible to the build.
export PERL5LIB="$dest/lib/perl5"
export PERL_MM_USE_DEFAULT=1 PERL_MM_OPT='' PERL_MB_OPT='' PERL_LOCAL_LIB_ROOT=''
unset PERL5OPT
jobs="${FLATPAK_BUILDER_N_JOBS:-1}"

for dist in "$@"; do
  folder="$sources/$dist"
  [[ -d "$folder" ]] || { printf 'missing distribution folder: %s\n' "$folder" >&2; exit 1; }
  printf 'build-compat-runtime: %s\n' "$dist"
  (
    cd "$folder"
    if [[ -f Makefile.PL ]]; then
      "$perl" Makefile.PL INSTALL_BASE="$dest" INSTALLMAN1DIR=none INSTALLMAN3DIR=none \
        INSTALLSITEMAN1DIR=none INSTALLSITEMAN3DIR=none </dev/null
      make -j"$jobs"
      make install
    elif [[ -f Build.PL ]]; then
      "$perl" Build.PL --install_base "$dest" --config installman1dir= --config installman3dir= \
        --config installsiteman1dir= --config installsiteman3dir= </dev/null
      "$perl" Build
      "$perl" Build install
    else
      printf '%s has neither Makefile.PL nor Build.PL\n' "$dist" >&2
      exit 1
    fi
  )
done

# Distributions install read-only, which the cleanup below, stripping and
# later copies trip on.
chmod -R u+w "$dest"
# Install records and documentation carry build times and paths; the worker
# hashes this tree to tell one runtime from another.
find "$dest" \( -name .packlist -o -name perllocal.pod -o -name '*.pod' \) -type f -delete
rm -rf "$dest/bin" "$dest/man"
find "$dest" -type d -empty -delete
