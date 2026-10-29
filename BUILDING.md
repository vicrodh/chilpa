# Building Chilpa from source

This page covers Linux. If you only want to use Chilpa, see [INSTALL.md](INSTALL.md).

## What you need

- Rust 1.96 or newer.
- Qt 6.11 or newer with Qt Quick. Release builds use 6.11.2.
- A C/C++ toolchain, CMake, pkg-config, libclang and NASM.
- Development packages for ALSA, JACK, PulseAudio, PipeWire, D-Bus, OpenSSL, OpenGL/EGL, xkbcommon and Wayland.

On Ubuntu 24.04:

```sh
sudo apt-get install -y --no-install-recommends \
  build-essential pkg-config cmake clang libclang-dev nasm \
  libasound2-dev libjack-jackd2-dev libpulse-dev libpipewire-0.3-dev \
  libdbus-1-dev libssl-dev libgl1-mesa-dev libegl1-mesa-dev \
  libxkbcommon-dev libwayland-dev libxkbcommon-x11-0 libxcb-cursor0 \
  python3-venv
```

If your distribution ships an older Qt, install 6.11.2 with aqtinstall:

```sh
python3 -m venv ~/.local/share/aqt
~/.local/share/aqt/bin/pip install aqtinstall==3.3.0
~/.local/share/aqt/bin/aqt install-qt linux desktop 6.11.2 linux_gcc_64 --outputdir ~/Qt

export QMAKE=~/Qt/6.11.2/gcc_64/bin/qmake
export CMAKE_PREFIX_PATH=~/Qt/6.11.2/gcc_64
export LD_LIBRARY_PATH=~/Qt/6.11.2/gcc_64/lib
```

## Build and run

```sh
git clone https://github.com/vicrodh/chilpa.git
cd chilpa
cargo build --locked --release -p chilpa-qt
./target/release/chilpa-qt
```

The plugin protocol is a vendored copy of the public SDK, so the build needs no other repository. `vendor/chilpa-plugin-api/ORIGIN.json` records the upstream revision.

## The exact release setup

[`.github/workflows/release-linux.yml`](.github/workflows/release-linux.yml) lists the packages used for published builds. [packaging/linux/README.md](packaging/linux/README.md) describes how the AppImage and the tarball are made.

## Contributing

Read [CONTRIBUTING.md](CONTRIBUTING.md) before opening a pull request.
