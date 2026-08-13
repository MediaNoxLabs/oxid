{ inputs, ... }:

{
  perSystem =
    { pkgs, self', ... }:
    let
      midnightDidPackages = inputs.midnight-did-toolchain.packages.${pkgs.stdenv.hostPlatform.system};
      presentationCompactArtifacts = pkgs.callPackage ./presentation-compact-artifacts.nix {
        compactMidnight = midnightDidPackages.compact-midnight;
        compactToolchain = midnightDidPackages.compact-toolchain;
        midnightCircuitParams = midnightDidPackages.midnight-circuit-params;
        midnightVcSource = inputs.midnight-verifiable-credentials;
      };
      passportVaultCompactArtifacts = pkgs.callPackage ./passport-vault-compact-artifacts.nix {
        compactMidnight = midnightDidPackages.compact-midnight;
        compactToolchain = midnightDidPackages.compact-toolchain;
        midnightCircuitParams = midnightDidPackages.midnight-circuit-params;
        midnightVcSource = inputs.midnight-verifiable-credentials;
        passportVaultSource = ../../contracts/passport-vault;
      };
      linuxBuildInputs = pkgs.lib.optionals pkgs.stdenv.hostPlatform.isLinux [
        pkgs.glib
        pkgs.gtk3
        pkgs.libsoup_3
        pkgs.webkitgtk_4_1
        pkgs.xdotool
      ];
      linuxNativeBuildInputs = pkgs.lib.optionals pkgs.stdenv.hostPlatform.isLinux [
        pkgs.wrapGAppsHook3
      ];
    in
    {
      packages = {
        default = pkgs.rustPlatform.buildRustPackage {
          pname = "oxid";
          version = "0.1.0";
          src = pkgs.lib.cleanSource ../..;

          cargoLock = {
            lockFile = ../../Cargo.lock;
            outputHashes = {
              "midnight-base-crypto-1.0.0" = "sha256-Sfl7vc9NpfdIZvXXYBQdg3VY5c35zMYwzHZcujxu8zY=";
            };
          };
          cargoBuildFlags = [
            "-p"
            "oxid-app"
          ];
          cargoTestFlags = [ "--workspace" ];

          nativeBuildInputs = [ pkgs.pkg-config ] ++ linuxNativeBuildInputs;
          buildInputs = [ pkgs.openssl ] ++ linuxBuildInputs;

          doCheck = true;
          strictDeps = true;

          meta = {
            description = "Rust-first identity-native wallet foundation";
            homepage = "https://github.com/MediaNoxLabs/oxid";
            license = pkgs.lib.licenses.asl20;
            mainProgram = "oxid-app";
          };
        };

        headless = pkgs.rustPlatform.buildRustPackage {
          pname = "oxid-headless";
          version = "0.1.0";
          src = pkgs.lib.cleanSource ../..;

          cargoLock = {
            lockFile = ../../Cargo.lock;
            outputHashes = {
              "midnight-base-crypto-1.0.0" = "sha256-Sfl7vc9NpfdIZvXXYBQdg3VY5c35zMYwzHZcujxu8zY=";
            };
          };
          cargoBuildFlags = [
            "-p"
            "oxid-headless"
          ];
          cargoTestFlags = [
            "-p"
            "oxid-headless"
          ];

          doCheck = true;
          strictDeps = true;

          meta = {
            description = "Headless Oxid wallet flow harness";
            homepage = "https://github.com/MediaNoxLabs/oxid";
            license = pkgs.lib.licenses.asl20;
            mainProgram = "oxid-headless";
          };
        };

        presentation-compact-artifacts = presentationCompactArtifacts;

        passport-vault-compact-artifacts = passportVaultCompactArtifacts;

        dioxus-cli = pkgs.dioxus-cli;
      }
      // pkgs.lib.optionalAttrs pkgs.stdenv.hostPlatform.isDarwin {
        xcodegen = pkgs.xcodegen;
      };

      checks.package = self'.packages.default;
      checks.headless = self'.packages.headless;
      checks.presentation-compact-artifacts = presentationCompactArtifacts;
      checks.passport-vault-compact-artifacts = passportVaultCompactArtifacts;
      formatter = pkgs.nixfmt;
    };
}
