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

      # What a distribution keeps under /usr and NixOS does not: the plugin
      # sandbox mounts this there, with its own bwrap, a loader and the
      # libraries a plugin built elsewhere expects to find.
      sandboxSystem = pkgs: pkgs.buildEnv {
        name = "chilpa-sandbox-system";
        paths = with pkgs; [
          bubblewrap util-linux perl bash coreutils gnused gnugrep
          glibc.out stdenv.cc.cc.lib openssl.out zlib.out
        ];
        pathsToLink = [ "/bin" "/lib" ];
        ignoreCollisions = true;
        postBuild = "ln -s lib $out/lib64";
      };

      # The Perl adapter for compatible plugins and the modules it loads.
      compatRuntime = pkgs:
        let
          modules = pkgs.buildEnv {
            name = "chilpa-compat-modules";
            paths = pkgs.perlPackages.requiredPerlModules (with pkgs.perlPackages; [
              URI HTTPMessage HTTPCookies XMLParser TemplateToolkit HTMLTree HTMLParser TextUnidecode
            ]);
            pathsToLink = [ "/lib/perl5" ];
          };
        in
        pkgs.runCommand "chilpa-compat-runtime" { } ''
          mkdir -p $out/adapter $out/libraries/lib/perl5
          cp -r ${./compat/perl}/. $out/adapter/
          # Real files: the worker's revision is a hash of this tree.
          cp -rL ${modules}/lib/perl5/site_perl/${pkgs.perl.version}/. $out/libraries/lib/perl5/
        '';

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

          qtWrapperArgs = [ "--set-default" "CHILPA_SANDBOX_SYSTEM" "${sandboxSystem pkgs}" ];

          postInstall = ''
            install -Dm644 packaging/linux/app.chilpa.Chilpa.desktop \
              $out/share/applications/app.chilpa.Chilpa.desktop
            install -Dm644 assets/logo.png \
              $out/share/icons/hicolor/512x512/apps/app.chilpa.Chilpa.png
            install -Dm644 LICENSE $out/share/licenses/chilpa/LICENSE
            cp -r licenses/. $out/share/licenses/chilpa/
          '';

          # Chilpa looks for the runtime beside its program. It goes in after
          # the fixup, which would wrap its modules as if they were programs.
          postFixup = ''
            cp -r ${compatRuntime pkgs} $out/bin/compat-runtime
          '';

          meta = {
            description = "Music player";
            homepage = "https://github.com/vicrodh/chilpa";
            license = pkgs.lib.licenses.mit;
            mainProgram = "chilpa-qt";
            platforms = systems;
          };
        };
        sandbox-system = sandboxSystem pkgs;
        compat-runtime = compatRuntime pkgs;
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
            export CHILPA_SANDBOX_SYSTEM=${sandboxSystem pkgs}
          '';
        };
      });

      formatter = forAllSystems (pkgs: pkgs.nixfmt);
    };
}
