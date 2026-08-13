{
  description = "Oxid identity wallet - reproducible Rust and Dioxus environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    midnight-did-toolchain.url = "github:midnightntwrk/midnight-did/05b237a5e51f9c22853b424e7d4236dfa9384c24";
    midnight-verifiable-credentials = {
      url = "github:midnightntwrk/midnight-verifiable-credentials/39b1354212620b396e914b29603e6a38f2656546";
      flake = false;
    };
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
