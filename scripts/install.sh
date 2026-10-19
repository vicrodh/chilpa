#!/usr/bin/env bash
# I install or update my user-owned Chilpa application from a verified release.
set -euo pipefail

chilpa_install() (
    fail() { printf 'Chilpa: %s\n' "$*" >&2; exit 1; }
    local requested='' check=0 arg
    for arg in "$@"; do
        case "$arg" in
            --check) check=1 ;;
            --version=*) requested=${arg#--version=} ;;
            --help)
                printf '%s\n' 'Usage: bash install.sh [--check] [--version=X.Y.Z[-prerelease]]' \
                    'I install the latest stable release, or the requested version.' \
                    'Run the same command again to update. Close Chilpa first.'
                return 0 ;;
            *) fail "Unknown option: $arg" ;;
        esac
    done
    [ "$(id -u)" -ne 0 ] || fail 'Run as your normal user, without sudo.'
    [ -n "${HOME:-}" ] && [[ "$HOME" = /* ]] && [ "$HOME" != / ] || fail 'HOME must be an absolute user directory.'
    case "$HOME" in *$'\n'*|*$'\r'*|*$'\t'*) fail 'HOME contains a control character.' ;; esac
    local system arch destination install_dir launcher='' receipt platform
    system=$(uname -s)
    arch=$(uname -m)
    case "$arch" in x86_64|amd64) arch=x86_64 ;; aarch64|arm64) arch=aarch64 ;; *) fail "Unsupported architecture: $arch" ;; esac
    case "$system" in
        Linux)
            local libc major minor
            libc=$(getconf GNU_LIBC_VERSION 2>/dev/null) || fail 'The official packages require glibc 2.39 or newer.'
            libc=${libc#glibc }
            IFS=. read -r major minor <<< "$libc"
            [[ "$major" =~ ^[0-9]+$ && "$minor" =~ ^[0-9]+$ ]] || fail 'Cannot determine the glibc version.'
            (( major > 2 || (major == 2 && minor >= 39) )) || fail 'The official packages require glibc 2.39 or newer.'
            platform=linux
            install_dir="$HOME/.local/opt/chilpa"
            destination="$install_dir/Chilpa.AppImage"
            launcher="$HOME/.local/bin/chilpa-qt" ;;
        Darwin)
            if [ "$(/usr/sbin/sysctl -n hw.optional.arm64 2>/dev/null || :)" = 1 ]; then arch=aarch64; fi
            platform=macos
            install_dir="$HOME/Applications"
            destination="$install_dir/Chilpa.app" ;;
        *) fail "Unsupported operating system: $system" ;;
    esac
    receipt="$install_dir/.chilpa-installer"
    local path owned=0 installed=''
    for path in "$install_dir" "$destination" "$receipt"; do
        [ ! -L "$path" ] || fail "Installation path is a symlink: $path"
    done
    if [ -f "$receipt" ] && [ "$(head -n 1 "$receipt")" = chilpa-installer-v1 ]; then
        owned=1
        installed=$(sed -n '2p' "$receipt")
    fi
    if [ -e "$destination" ] && [ "$owned" = 0 ]; then fail "Existing installation preserved: $destination"; fi
    if [ "$system" = Linux ] && { [ -e "$launcher" ] || [ -L "$launcher" ]; }; then
        [ "$owned" = 1 ] && [ ! -L "$launcher" ] && grep -q '^# Chilpa installer v1$' "$launcher" || fail "Existing launcher preserved: $launcher"
    fi
    command -v curl >/dev/null 2>&1 || fail 'curl is required.'
    if [ "$system" != Darwin ] && ! command -v python3 >/dev/null 2>&1 && ! command -v jq >/dev/null 2>&1; then
        fail 'Install python3 or jq, then run this command again.'
    fi
    local scratch stage='' lock='' previous='' committed=0
    scratch=$(mktemp -d "${TMPDIR:-/tmp}/chilpa-install.XXXXXXXX")
    cleanup() {
        # I restore the existing app when replacement fails before committing.
        if [ "$committed" = 0 ] && [ -n "$previous" ] && [ -e "$previous" ] && [ ! -e "$destination" ]; then mv "$previous" "$destination"; fi
        [ -z "$stage" ] || rm -rf -- "$stage"
        rm -rf -- "$scratch"
        [ -z "$lock" ] || rmdir "$lock"
    }
    trap cleanup EXIT
    trap 'exit 130' INT
    trap 'exit 143' TERM
    fetch() { curl --proto '=https' --proto-redir '=https' --tlsv1.2 --fail --location --silent --show-error --connect-timeout 10 --max-time 600 --max-filesize 2147483648 --retry 2 --output "$2" "$1"; }
    local api='https://api.github.com/repos/vicrodh/chilpa/releases/latest'
    if [ -n "$requested" ]; then
        [[ "$requested" =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-[0-9A-Za-z-]+(\.[0-9A-Za-z-]+)*)?$ ]] || fail 'Invalid --version.'
        api="https://api.github.com/repos/vicrodh/chilpa/releases/tags/v$requested"
    fi
    fetch "$api" "$scratch/release.json" || fail 'Cannot read the release. Check your connection or GitHub rate limit.'
    json_get() {
        if [ "$system" = Darwin ] && [ -x /usr/bin/osascript ]; then
            /usr/bin/osascript -l JavaScript - "$scratch/release.json" "$1" <<'JS'
ObjC.import('Foundation');
function run(args) {
    const data = $.NSData.dataWithContentsOfFile(args[0]);
    let value = JSON.parse(ObjC.unwrap($.NSString.alloc.initWithDataEncoding(data, $.NSUTF8StringEncoding)));
    args[1].split('.').forEach(function (key) { value = value[key]; });
    if (Array.isArray(value)) return String(value.length);
    if (['string', 'boolean', 'number'].indexOf(typeof value) < 0) throw Error('Unsupported release field');
    return String(value);
}
JS
        elif command -v python3 >/dev/null 2>&1; then
            python3 - "$scratch/release.json" "$1" <<'PY'
import json, sys
with open(sys.argv[1]) as stream:
    value = json.load(stream)
for key in sys.argv[2].split('.'):
    value = value[int(key)] if isinstance(value, list) else value[key]
if isinstance(value, list): print(len(value))
elif isinstance(value, bool): print(str(value).lower())
elif isinstance(value, (str, int)): print(value)
else: raise SystemExit('Unsupported release field')
PY
        else
            jq -er --arg path "$1" 'getpath($path | split(".") | map(tonumber? // .)) | if type == "array" then length elif type == "boolean" then tostring else . end' "$scratch/release.json"
        fi
    }
    local tag version filename count i url='' checksum='' size='' name
    tag=$(json_get tag_name)
    [[ "$tag" =~ ^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-[0-9A-Za-z-]+(\.[0-9A-Za-z-]+)*)?$ ]] || fail 'Invalid release tag.'
    version=${tag#v}
    [ "$(json_get draft)" = false ] || fail 'Draft releases cannot be installed.'
    if [ -z "$requested" ]; then
        [ "$(json_get prerelease)" = false ] && [[ "$version" != *-* ]] || fail 'The latest release is not stable.'
        if [ -n "$installed" ]; then
            [[ "$installed" =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-[0-9A-Za-z-]+(\.[0-9A-Za-z-]+)*)?$ ]] || fail 'Invalid installer receipt.'
            local old_major old_minor old_patch new_major new_minor new_patch
            IFS=. read -r old_major old_minor old_patch <<< "${installed%%-*}"
            IFS=. read -r new_major new_minor new_patch <<< "$version"
            if (( old_major > new_major || (old_major == new_major && old_minor > new_minor) || (old_major == new_major && old_minor == new_minor && old_patch > new_patch) )); then
                fail 'The installed version is newer. Use --version only to request an explicit downgrade.'
            fi
        fi
    else
        [ "$version" = "$requested" ] || fail 'The response differs from the requested version.'
    fi
    if [ "$system" = Linux ]; then filename="Chilpa-$version-$arch.AppImage"; else filename="Chilpa-$version-macos-$arch.zip"; fi
    count=$(json_get assets)
    [[ "$count" =~ ^[0-9]+$ ]] && [ "$count" -le 1000 ] || fail 'Invalid release asset count.'
    for ((i=0; i<count; i++)); do
        name=$(json_get "assets.$i.name")
        if [ "$name" = "$filename" ]; then
            [ -z "$url" ] || fail 'Duplicate release package.'
            url=$(json_get "assets.$i.browser_download_url")
            checksum=$(json_get "assets.$i.digest")
            size=$(json_get "assets.$i.size")
        fi
    done
    [ "$url" = "https://github.com/vicrodh/chilpa/releases/download/$tag/$filename" ] || fail "No official package for $platform-$arch."
    [[ "$checksum" =~ ^sha256:[0-9a-f]{64}$ ]] || fail 'The release has no SHA-256 digest.'
    [[ "$size" =~ ^[0-9]+$ ]] && [ "$size" -gt 0 ] && [ "$size" -le 2147483648 ] || fail 'Invalid package size.'
    printf 'Chilpa %s (%s-%s)\nDestination: %s\n' "$version" "$platform" "$arch" "$destination"
    if [ "$check" = 1 ]; then printf 'Package: %s\nSHA-256: %s\nInstalled: %s\n' "$url" "${checksum#sha256:}" "${installed:-none}"; return 0; fi
    if command -v pgrep >/dev/null 2>&1 && pgrep -u "$(id -u)" -x chilpa-qt >/dev/null 2>&1; then fail 'Close Chilpa before installing or updating.'; fi
    fetch "$url" "$scratch/package" || fail 'Download failed. The installation was preserved.'
    local actual
    if command -v sha256sum >/dev/null 2>&1; then actual=$(sha256sum "$scratch/package"); else actual=$(shasum -a 256 "$scratch/package"); fi
    [ "${actual%% *}" = "${checksum#sha256:}" ] || fail 'Package checksum mismatch. The installation was preserved.'
    [ "$(wc -c < "$scratch/package" | tr -d ' ')" = "$size" ] || fail 'Package size mismatch.'
    mkdir -p "$install_dir"
    mkdir "$install_dir/.chilpa-install.lock" 2>/dev/null || fail 'Another installer is active.'
    lock="$install_dir/.chilpa-install.lock"
    stage=$(mktemp -d "$install_dir/.chilpa-install.XXXXXXXX")
    for path in "$destination" "$receipt"; do
        [ ! -L "$path" ] || fail "Installation changed while downloading: $path"
    done
    if [ -e "$destination" ]; then
        [ "$owned" = 1 ] && [ -f "$receipt" ] && [ "$(head -n 1 "$receipt")" = chilpa-installer-v1 ] || fail 'An installation appeared while downloading.'
        [ "$(sed -n '2p' "$receipt")" = "$installed" ] || fail 'The installed version changed while downloading.'
    fi
    if [ "$system" = Linux ]; then
        [ "$(od -An -tx1 -N4 "$scratch/package" | tr -d ' \n')" = 7f454c46 ] || fail 'The package is not ELF.'
        [ "$(od -An -tx1 -j8 -N3 "$scratch/package" | tr -d ' \n')" = 414902 ] || fail 'The package is not a type-2 AppImage.'
        local machine
        machine=$(od -An -tx1 -j18 -N2 "$scratch/package" | tr -d ' \n')
        if [ "$arch" = x86_64 ]; then [ "$machine" = 3e00 ] || fail 'Wrong ELF architecture.'; else [ "$machine" = b700 ] || fail 'Wrong ELF architecture.'; fi
        cp "$scratch/package" "$stage/Chilpa.AppImage"
        chmod 755 "$stage/Chilpa.AppImage"
        mkdir -p "$HOME/.local/bin"
        {
            printf '%s\n' '#!/usr/bin/env bash' '# Chilpa installer v1' 'export APPIMAGE_EXTRACT_AND_RUN=1'
            printf 'exec %q "$@"\n' "$destination"
        } > "$stage/launcher"
        chmod 755 "$stage/launcher"
        # I publish a complete image by rename; the application adds its menu entry on first launch.
        mv -f "$stage/Chilpa.AppImage" "$destination"
        mv -f "$stage/launcher" "$launcher"
    else
        mkdir "$stage/unpacked"
        /usr/bin/ditto -x -k "$scratch/package" "$stage/unpacked"
        local app="$stage/unpacked/Chilpa.app" minimum current
        [ -f "$app/Contents/MacOS/chilpa-qt" ] && [ -f "$app/Contents/Info.plist" ] || fail 'Invalid application bundle.'
        /usr/bin/codesign --verify --deep --strict "$app" || fail 'The app signature failed validation.'
        minimum=$(/usr/libexec/PlistBuddy -c 'Print LSMinimumSystemVersion' "$app/Contents/Info.plist")
        current=$(sw_vers -productVersion)
        local required_major required_minor current_major current_minor
        IFS=. read -r required_major required_minor _ <<< "$minimum"
        IFS=. read -r current_major current_minor _ <<< "$current"
        (( current_major > required_major || (current_major == required_major && current_minor >= required_minor) )) || fail "This package requires macOS $minimum."
        if [ -e "$destination" ]; then
            previous="$stage/previous.app"
            mv "$destination" "$previous"
        fi
        mv "$app" "$destination"
    fi
    printf 'chilpa-installer-v1\n%s\n' "$version" > "$stage/receipt"
    mv -f "$stage/receipt" "$receipt"
    committed=1
    printf '%s\n' 'Chilpa installed. Run this command again to update.'
    if [ "$system" = Linux ]; then printf 'Launch: %s\n' "$launcher"; else printf 'Launch: %s\n' "$destination"; fi
)
chilpa_install "$@"
