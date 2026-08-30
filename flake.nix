{
  description = "Rust + egui dev shell with FRR and Docker";

  inputs = {
    nixpkgs.url      = "github:NixOS/nixpkgs/nixos-26.05";
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
          libxkbcommon
          libx11
          libxcb
          libxcursor
          libxi
          libxrandr
          libxxf86vm
          libxinerama
          libxext
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
            nixd
            act
            openssl.dev
            pkg-config
            direnv
            net-snmp
            libxkbcommon
            libx11
            libxcb
            libxcursor
            libxi
            libxrandr
            libxxf86vm
            libxinerama
            libxext
            wayland
            wayland-protocols
            vulkan-loader
            libGL
            perf
            containerlab
            python314            
            gnmic

            # libwayland  # Uncomment if you need the static lib
            # nushell     # Uncomment to use nushell as login shell
            # u-config    # Uncomment if you want this config tool
          ];

          OPENSSL_DIR = "${pkgs.openssl.dev}";
          OPENSSL_LIB_DIR = "${pkgs.openssl.out}/lib";
          OPENSSL_INCLUDE_DIR = "${pkgs.openssl.dev}/include";
          LD_LIBRARY_PATH = "${pkgs.openssl.out}/lib:${libPath}";

        };
      }
    );
}
