{
  description = "A basic Rust project";

  inputs = {
    nixpkgs-src.url = "https://github.com/NixOS/nixpkgs/archive/nixos-25.05.tar.gz";
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
            pkgs.rustc
            pkgs.cargo
          ];
        };
      });
}