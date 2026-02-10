{
  description = "ordne - Safe File Deduplication, Classification & Migration";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rust-analyzer" ];
        };
      in
      {
        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            rustToolchain
            pkg-config
            sqlite

            # External tools that ordne wraps
            rmlint
            rsync
            rclone

            # Development tools
            cargo-watch
            cargo-expand
            cargo-audit
          ];

          shellHook = ''
            echo "ordne development environment"
            echo "Tools available: rmlint, rsync, rclone"
            echo "Rust version: $(rustc --version)"
          '';

          RUST_BACKTRACE = "1";
        };

        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "ordne";
          version = "0.1.0";
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;

          nativeBuildInputs = with pkgs; [ pkg-config ];
          buildInputs = with pkgs; [ sqlite ];

          meta = with pkgs.lib; {
            description = "Safe file deduplication, classification and migration tool";
            homepage = "https://github.com/youruser/ordne";
            license = licenses.mit;
            maintainers = [ ];
          };
        };
      }
    );
}
