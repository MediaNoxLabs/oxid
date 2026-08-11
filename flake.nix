{
  description = "Oxid identity wallet - reproducible Rust and Dioxus environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
  };

  outputs =
    inputs@{ flake-parts, ... }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      imports = [
        ./nix/packages
        ./nix/devshells
      ];
      systems = [
        "x86_64-linux"
        "aarch64-darwin"
      ];
    };
}
