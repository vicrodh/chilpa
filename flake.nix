{
  description = "Chilpa music player";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
      cargoToml = builtins.fromTOML (builtins.readFile ./Cargo.toml);

      # nixpkgs splits Qt across packages; cxx-qt asks one qmake for every tool,
      # so it needs a merged prefix that also holds qtdeclarative's libexec.
      qtEnv = pkgs: with pkgs.qt6; env "chilpa-qt-env" [ qtdeclarative qtwayland qtsvg ];

      nativeDeps = pkgs: with pkgs; [
        pkg-config
        cmake
        nasm
        perl # vendored OpenSSL
        rustPlatform.bindgenHook
        qt6.wrapQtAppsHook
      ];

      buildDeps = pkgs: with pkgs; [
        alsa-lib
        libjack2
        libpulseaudio
        pipewire
        dbus
        openssl
        libGL
        libxkbcommon
        wayland
        qt6.qtbase
        qt6.qtdeclarative
        qt6.qtwayland
        qt6.qtsvg
      ];
    in
    {
      packages = forAllSystems (pkgs: rec {
        chilpa = pkgs.rustPlatform.buildRustPackage {
          pname = "chilpa";
          version = cargoToml.workspace.package.version;

          src = pkgs.lib.cleanSourceWith {
            src = ./.;
            filter = path: type:
              let base = baseNameOf path; in
              !(builtins.elem base [ "target" ".jj" ".git" "flake.nix" "flake.lock" "result" ]);
          };

          cargoLock.lockFile = ./Cargo.lock;

          nativeBuildInputs = nativeDeps pkgs;
          buildInputs = buildDeps pkgs;

          # qtbase's setup hook exports its own QMAKE, so set ours after it.
          preConfigure = ''
            export QMAKE=${qtEnv pkgs}/bin/qmake
          '';

          cargoBuildFlags = [ "-p" "chilpa-qt" ];
          # The workspace tests need audio devices, a display and the network.
          doCheck = false;

          postInstall = ''
            install -Dm644 packaging/linux/app.chilpa.Chilpa.desktop \
              $out/share/applications/app.chilpa.Chilpa.desktop
            install -Dm644 assets/logo.png \
              $out/share/icons/hicolor/512x512/apps/app.chilpa.Chilpa.png
            install -Dm644 LICENSE $out/share/licenses/chilpa/LICENSE
            cp -r licenses/. $out/share/licenses/chilpa/
          '';

          meta = {
            description = "Music player";
            homepage = "https://github.com/vicrodh/chilpa";
            license = pkgs.lib.licenses.mit;
            mainProgram = "chilpa-qt";
            platforms = systems;
          };
        };
        default = chilpa;
      });

      apps = forAllSystems (pkgs: {
        default = {
          type = "app";
          meta.description = "Run Chilpa";
          program = "${self.packages.${pkgs.stdenv.hostPlatform.system}.chilpa}/bin/chilpa-qt";
        };
      });

      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
          inputsFrom = [ self.packages.${pkgs.stdenv.hostPlatform.system}.chilpa ];
          packages = with pkgs; [ cargo rustc clippy rustfmt rust-analyzer bubblewrap python3 ];
          shellHook = ''
            export QMAKE=${qtEnv pkgs}/bin/qmake
          '';
        };
      });

      formatter = forAllSystems (pkgs: pkgs.nixfmt);
    };
}
