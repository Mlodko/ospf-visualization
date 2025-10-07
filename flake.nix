{
  description = "A basic Rust project";

  inputs = {
    nixpkgs-src.url = "https://github.com/NixOS/nixpkgs/archive/nixos-unstable.tar.gz";
    flake-utils-src.url = "https://github.com/numtide/flake-utils/archive/main.tar.gz";
  };

  outputs = { self, nixpkgs-src, flake-utils-src }:
    flake-utils-src.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs-src.legacyPackages.${system};
      in
      {
        devShells.default = pkgs.mkShell {
          buildInputs = [
            pkgs.docker
            pkgs.docker-compose
            pkgs.nixd
            pkgs.act
            pkgs.rustc
            pkgs.rust-analyzer
            pkgs.rustfmt
            pkgs.cargo
            pkgs.openssl.dev
            pkgs.pkg-config
            pkgs.direnv
            pkgs.net-snmp
          ];
          OPENSSL_DIR = "${pkgs.openssl.dev}";
          OPENSSL_LIB_DIR = "${pkgs.openssl.out}/lib";
          OPENSSL_INCLUDE_DIR = "${pkgs.openssl.dev}/include";
          LD_LIBRARY_PATH = "${pkgs.openssl.out}/lib:${pkgs.lib.makeLibraryPath [pkgs.openssl]}";
          
          shellHook = ''
            echo "🚀 Starting FRR container..."
            docker-compose up -d
            echo "✅ FRR running at 172.20.0.10 (SNMP on localhost:161)"
            echo "🔧 Test: snmpwalk -v2c -c public localhost:161 1.3.6.1.2.1.1"
            trap 'docker-compose down' EXIT
          '';
        };
      });
  
}
