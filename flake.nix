{
  description = "A fast, keyboard-first file manager for Linux";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
    }:
    let
      inherit (nixpkgs) lib;
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      # Keep in lockstep with tests/e2e/Dockerfile (RUSTUP_TOOLCHAIN).
      rustVersion = "1.98.1";
      cargoToml = builtins.fromTOML (builtins.readFile ./Cargo.toml);

      forEachSystem = f: lib.genAttrs systems (system: f (pkgsFor system));
      pkgsFor =
        system:
        import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        };

      rustToolchain =
        pkgs:
        pkgs.rust-bin.stable.${rustVersion}.minimal.override {
          extensions = [
            "clippy"
            "rust-analyzer"
            "rust-src"
            "rustfmt"
          ];
        };

      commonNativeBuildInputs = pkgs: [
        pkgs.glib
        pkgs.pkg-config
      ];

      commonBuildInputs = pkgs: [
        pkgs.cairo
        pkgs.fontconfig
        pkgs.gdk-pixbuf
        pkgs.gtk4
        pkgs.gtksourceview5
        pkgs.pango
        pkgs.poppler
      ];
    in
    {
      packages = forEachSystem (
        pkgs:
        let
          rust = rustToolchain pkgs;
          rustPlatform = pkgs.makeRustPlatform {
            cargo = rust;
            rustc = rust;
          };
        in
        {
          default = rustPlatform.buildRustPackage {
            pname = "strata";
            version = cargoToml.package.version;
            src = lib.fileset.toSource {
              root = ./.;
              fileset = lib.fileset.unions [
                ./Cargo.toml
                ./Cargo.lock
                ./build.rs
                ./src
                ./data
              ];
            };
            cargoLock.lockFile = ./Cargo.lock;
            strictDeps = true;
            nativeBuildInputs = commonNativeBuildInputs pkgs ++ [ pkgs.wrapGAppsHook4 ];
            buildInputs = commonBuildInputs pkgs;
            env.STRATA_BUILD_COMMIT = self.shortRev or self.dirtyShortRev or "unknown";
            doCheck = false;
            meta = {
              description = cargoToml.package.description;
              homepage = cargoToml.package.repository;
              license = lib.licenses.gpl3Plus;
              platforms = systems;
              mainProgram = "strata";
            };
          };
        }
      );

      devShells = forEachSystem (
        pkgs:
        let
          rust = rustToolchain pkgs;
        in
        {
          default = pkgs.mkShell {
            buildInputs = commonBuildInputs pkgs;
            nativeBuildInputs = commonNativeBuildInputs pkgs ++ [
              rust
              pkgs.bubblewrap
              pkgs.cargo-deny
              pkgs.cargo-watch
              pkgs.git
              pkgs.gsettings-desktop-schemas
              pkgs.hicolor-icon-theme
              pkgs.python3
              pkgs.shared-mime-info
              pkgs.typos
            ];
            env.RUST_SRC_PATH = "${rust}/lib/rustlib/src/rust/library";
          };
        }
      );
    };
}
