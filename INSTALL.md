# Installing Chilpa

The README has the one-line install. This page covers the other formats and the details of each. Everything is on the [Releases](https://github.com/vicrodh/chilpa/releases) page.

## The install script

```sh
curl -fsSL --proto '=https' https://chilpa.app/install.sh | bash
```

- Linux: installs into `~/.local/opt/chilpa`. Needs glibc 2.39 or newer, curl, and Python 3 or jq.
- macOS: installs `~/Applications/Chilpa.app`. Uses the system's JavaScript runtime, so nothing extra is needed.
- To update, close Chilpa and run the same command again.
- Before replacing an installation it made, the script checks the release's SHA-256 digest and size. It leaves installations from a package manager alone.
- To see which package it would pick without installing, add `-s -- --check` after `bash`.
- To install a prerelease, download `install.sh` from that release and run `bash install.sh --version=X.Y.Z-beta.N`.

On macOS, if the system refuses to open the app the first time, remove the quarantine flag once and open it again:

```sh
xattr -dr com.apple.quarantine ~/Applications/Chilpa.app
```

## DEB and RPM

Download the package for your CPU, then:

```sh
sudo apt install ./chilpa_0.1.1_amd64.deb
# Fedora:
sudo dnf install ./chilpa-0.1.1-1.x86_64.rpm
```

For ARM64 use `arm64.deb` or `aarch64.rpm`. The packages include the Qt runtime under `/opt/chilpa`. Update them with your package manager.

## Package repositories

A signed repository at `pkgs.chilpa.app` brings every new release through `apt upgrade` and `dnf upgrade`. It serves the indices only; the packages come from the GitHub release.

Debian, Ubuntu and derivatives (amd64, arm64):

```sh
sudo curl -fsSL https://pkgs.chilpa.app/chilpa.gpg -o /usr/share/keyrings/chilpa.gpg
echo 'deb [signed-by=/usr/share/keyrings/chilpa.gpg] https://pkgs.chilpa.app/apt stable main' \
  | sudo tee /etc/apt/sources.list.d/chilpa.list
sudo apt update && sudo apt install chilpa
```

Fedora, RHEL and derivatives (x86_64, aarch64):

```sh
sudo curl -fsSL https://pkgs.chilpa.app/chilpa.repo -o /etc/yum.repos.d/chilpa.repo
sudo dnf install chilpa
```

openSUSE:

```sh
sudo zypper addrepo https://pkgs.chilpa.app/chilpa.repo
sudo zypper --gpg-auto-import-keys refresh && sudo zypper install chilpa
```

The repository metadata is signed with a key made only for Chilpa packages. The armored public key is `https://pkgs.chilpa.app/chilpa.asc` (the binary form is `chilpa.gpg`). To read its fingerprint before trusting it:

```sh
curl -fsSL https://pkgs.chilpa.app/chilpa.asc | gpg --show-keys --fingerprint
```

`chilpa.repo` turns on `repo_gpgcheck`, so dnf and zypper reject metadata that does not match the key. The RPM files themselves are not signed individually, so it keeps `gpgcheck=0`; their SHA-256 sums are in the release's `SHA256SUMS`.

## Arch Linux and Gentoo

```sh
# Arch (AUR)
yay -S chilpa-bin

# Gentoo
eselect repository add chilpa git https://github.com/vicrodh/chilpa-overlay.git
emerge --sync chilpa && emerge media-sound/chilpa-bin
```

## AppImage

Download `Chilpa-0.1.1-x86_64.AppImage` and `SHA256SUMS`, then:

```sh
sha256sum --check --ignore-missing SHA256SUMS
chmod +x Chilpa-0.1.1-x86_64.AppImage
./Chilpa-0.1.1-x86_64.AppImage
```

The image carries its own Qt and audio runtime. If FUSE is not available, start it with `APPIMAGE_EXTRACT_AND_RUN=1`.

**Menu entry.** On every start Chilpa writes `~/.local/share/applications/app.chilpa.Chilpa.desktop` and its icon, with `Exec` pointing at the image, so a moved image is followed the next time it starts. If AppImageLauncher, Gear Lever or a similar tool already added the image, Chilpa adds no second entry. It never replaces an entry it did not write. To remove it, delete that file and `~/.local/share/icons/hicolor/512x512/apps/app.chilpa.Chilpa.png`.

**Updates.** Keep the image in a folder you own, such as `~/Applications`. Chilpa checks for updates once a day. About has the switch for the automatic check and a Check for updates button. An update downloads next to the image, is verified against the release checksum, and replaces the file, keeping the old one as `<name>.previous`. Version 0.1.1 uses the stable update channel.

## Tarball

```sh
tar xf chilpa-0.1.1-linux-x86_64.tar.gz
./chilpa-0.1.1/chilpa-qt
```

The tarball uses your system libraries: Qt 6.11 or newer with Qt Quick, ALSA, PulseAudio, D-Bus, SQLite and OpenSSL 3. Chilpa updates it in place: About has the switch for the automatic check and a Check for updates button. The folder keeps its name and path, and the previous version stays beside it as `<folder>.previous`. Keep the folder where you can write to it.

It includes a desktop entry and an icon. The entry runs `/usr/bin/chilpa-qt`, which suits a package. To add Chilpa to your own menu, run this from the folder where you extracted it:

```sh
mkdir -p ~/.local/share/applications
sed "s|^Exec=.*|Exec=$PWD/chilpa-0.1.1/chilpa-qt %U|" \
  chilpa-0.1.1/app.chilpa.Chilpa.desktop \
  > ~/.local/share/applications/app.chilpa.Chilpa.desktop
cp -r chilpa-0.1.1/icons ~/.local/share/
```

## macOS

Download the DMG or the ZIP: `macos-aarch64` for Apple Silicon, `macos-x86_64` for Intel. Open the DMG and drag Chilpa to Applications, or unzip and move `Chilpa.app` there. Both include Qt. The release manifest records the minimum macOS version.

The app is signed ad hoc and is not notarized, so macOS may refuse to open it. Remove the quarantine flag:

```sh
xattr -dr com.apple.quarantine /Applications/Chilpa.app
```

Use `~/Applications/Chilpa.app` if the install script put it there. You can also approve the app in System Settings > Privacy & Security.

Chilpa updates the app in place: About has the switch for the automatic check and a Check for updates button, and the previous version stays beside the app as `Chilpa.app.previous`. Running the install script again also works. A copy managed by Homebrew or signed by someone else is updated through its own channel.

## Uninstalling

Remove the application:

| Installed with | Remove |
| --- | --- |
| Install script, Linux | `rm -rf ~/.local/opt/chilpa` |
| Install script, macOS | `rm -rf ~/Applications/Chilpa.app` |
| DEB | `sudo apt remove chilpa` |
| RPM | `sudo dnf remove chilpa` |
| AppImage | delete the `.AppImage`, then the menu entry files listed under AppImage above |
| Tarball | delete the extracted folder |
| macOS, by hand | delete `Chilpa.app` from Applications |

Removing the application leaves your data on disk.

<details>
<summary>Delete your data</summary>

Linux:

```sh
rm -rf ~/.config/chilpa ~/.local/share/chilpa ~/.cache/chilpa
```

macOS:

```sh
rm -rf ~/Library/Application\ Support/chilpa ~/Library/Caches/chilpa
```

</details>

## Where your data lives

| What | Linux | macOS |
| --- | --- | --- |
| Settings | `~/.config/chilpa` | `~/Library/Application Support/chilpa` |
| Library and plugins | `~/.local/share/chilpa` | `~/Library/Application Support/chilpa` |
| Artwork cache | `~/.cache/chilpa` | `~/Library/Caches/chilpa` |

If `XDG_CONFIG_HOME`, `XDG_DATA_HOME` or `XDG_CACHE_HOME` is set, Chilpa uses it on both systems.
