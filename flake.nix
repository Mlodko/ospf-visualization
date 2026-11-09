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
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };
        lib = pkgs.lib;
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
            (rust-bin.stable.latest.default.override {
	      extensions = [ "rust-src" ];
	    })
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
            perf
            protobuf
            protobufc
            containerlab
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
