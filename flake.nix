{
  description = "Rust + egui dev shell with FRR and Docker";

  inputs = {
    nixpkgs.url      = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url  = "github:numtide/flake-utils";
    fenix.url        = "github:nix-community/fenix";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs = { nixpkgs, flake-utils, fenix, rust-overlay, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ fenix.overlays.default rust-overlay.overlays.default ];
        };
        lib = pkgs.lib;

        stableToolchain = fenix.packages.${system}.complete.toolchain;
        rustAnalyzer    = fenix.packages.${system}.latest.rust-analyzer;
        libPath = with pkgs; lib.makeLibraryPath [
          wayland-protocols
          wayland
          libxkbcommon
          libGL
          xorg.libX11
          xorg.libxcb
          xorg.libXcursor
          xorg.libXi
          xorg.libXrandr
          xorg.libXxf86vm
          xorg.libXinerama
          xorg.libXext
          vulkan-loader
          mesa
          vulkan-tools
          libdrm
          libllvm
        ];
      in {
        devShells.default = pkgs.mkShell {
          name = "rust-egui-dev-shell";

          buildInputs = with pkgs; lib.flatten [
            stableToolchain
            rustAnalyzer
            rustfmt
            cargo
            cargo-expand
            docker
            docker-compose
            nixd
            act
            openssl.dev
            pkg-config
            direnv
            net-snmp
            libxkbcommon
            xorg.libX11
            xorg.libxcb
            xorg.libXcursor
            xorg.libXi
            xorg.libXrandr
            xorg.libXxf86vm
            xorg.libXinerama
            xorg.libXext
            wayland
            wayland-protocols
            vulkan-loader
            libGL
            
            # libwayland  # Uncomment if you need the static lib
            # nushell     # Uncomment to use nushell as login shell
            # u-config    # Uncomment if you want this config tool
          ];

          OPENSSL_DIR = "${pkgs.openssl.dev}";
          OPENSSL_LIB_DIR = "${pkgs.openssl.out}/lib";
          OPENSSL_INCLUDE_DIR = "${pkgs.openssl.dev}/include";
          LD_LIBRARY_PATH = "${pkgs.openssl.out}/lib:${libPath}";

          shellHook = ''
            echo "🚀 Starting FRR container..."
            docker compose -f ./router_configs/docker-compose.yml up -d --build
            echo "✅ FRR running at 172.20.0.10 (SNMP on localhost:161)"
            echo "🔧 Test: snmpwalk -v2c -c public localhost:161 1.3.6.1.2.1.1"
            trap 'docker compose -f ./router_configs/docker-compose.yml down' EXIT

            echo "Using Rust toolchain: $(rustc --version)"
            export CARGO_HOME="$HOME/.cargo"
            export RUSTUP_HOME="$HOME/.rustup"
            export LD_LIBRARY_PATH="${libPath}:${pkgs.openssl.out}/lib:$LD_LIBRARY_PATH"
            mkdir -p "$CARGO_HOME" "$RUSTUP_HOME"
            
            export WINIT_UNIX_BACKEND=x11

            # Uncomment to launch nushell as login shell
            # exec nu --login
          '';
        };
      }
    );
}
