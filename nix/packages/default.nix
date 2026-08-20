{ inputs, ... }:

{
  perSystem =
    { pkgs, self', ... }:
    let
      arrayrefOutputHash = "sha256-INuaZ5B8eEFnizANKFvSsHnBmQGoSEo1Jvxo1RLWxLY=";
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
      passportVaultCallComposer = pkgs.callPackage ./passport-vault-call-composer.nix {
        inherit passportVaultCompactArtifacts;
        vaultContractStateFixture = ../../fixtures/passport-vault/contract-state-v1.hex;
      };
      oxidApp = pkgs.rustPlatform.buildRustPackage {
        pname = "oxid";
        version = "0.1.0";
        src = pkgs.lib.cleanSource ../..;

        cargoLock = {
          lockFile = ../../Cargo.lock;
          outputHashes = {
            "arrayref-0.3.9" = arrayrefOutputHash;
            "midnight-base-crypto-1.0.0" = "sha256-Sfl7vc9NpfdIZvXXYBQdg3VY5c35zMYwzHZcujxu8zY=";
          };
        };
        cargoBuildFlags = [
          "-p"
          "oxid-app"
        ];
        cargoTestFlags = [ "--workspace" ];
        OXID_PASSPORT_VAULT_ARTIFACTS_DIR = passportVaultCompactArtifacts;
        OXID_PASSPORT_VAULT_COMPOSER = "${passportVaultCallComposer}/bin/oxid-passport-vault-call-composer";

        nativeBuildInputs = [ pkgs.pkg-config ] ++ linuxNativeBuildInputs;
        buildInputs = [ pkgs.openssl ] ++ linuxBuildInputs;

        # Per-push CI verifies tests once in the repository gate; the hermetic
        # re-run lives in the checked variants below, exercised by
        # `nix flake check` and the nightly workflow.
        doCheck = false;
        strictDeps = true;

        meta = {
          description = "Rust-first identity-native wallet foundation";
          homepage = "https://github.com/MediaNoxLabs/oxid";
          license = pkgs.lib.licenses.asl20;
          mainProgram = "oxid-app";
        };
      };
      brandCheck = pkgs.rustPlatform.buildRustPackage {
        pname = "oxid-brand-check";
        version = "0.1.0";
        src = pkgs.lib.cleanSource ../..;

        cargoLock = {
          lockFile = ../../Cargo.lock;
          outputHashes = {
            "arrayref-0.3.9" = arrayrefOutputHash;
            "midnight-base-crypto-1.0.0" = "sha256-Sfl7vc9NpfdIZvXXYBQdg3VY5c35zMYwzHZcujxu8zY=";
          };
        };
        cargoBuildFlags = [
          "-p"
          "oxid-brand-build"
          "--bin"
          "oxid-brand-check"
        ];
        cargoTestFlags = [
          "-p"
          "oxid-brand-build"
        ];
        strictDeps = true;

        meta = {
          description = "Validated Oxid build-time brand-pack checker";
          homepage = "https://github.com/MediaNoxLabs/oxid";
          license = pkgs.lib.licenses.asl20;
          mainProgram = "oxid-brand-check";
        };
      };
      brandDirectories = pkgs.lib.filterAttrs (_name: kind: kind == "directory") (
        builtins.readDir ../../brands
      );
      brandChecks = pkgs.lib.mapAttrs' (
        name: _kind:
        pkgs.lib.nameValuePair "brand-${name}" (
          pkgs.runCommand "oxid-brand-${name}-check" { nativeBuildInputs = [ brandCheck ]; } ''
            oxid-brand-check ${../../brands}/${name}
            touch $out
          ''
        )
      ) brandDirectories;
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
        default = oxidApp;

        oxid-app-oxid = oxidApp;

        brand-check = brandCheck;

        headless = pkgs.rustPlatform.buildRustPackage {
          pname = "oxid-headless";
          version = "0.1.0";
          src = pkgs.lib.cleanSource ../..;

          cargoLock = {
            lockFile = ../../Cargo.lock;
            outputHashes = {
              "arrayref-0.3.9" = arrayrefOutputHash;
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

          # See packages.default: hermetic tests run in the checked variant.
          doCheck = false;
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

        passport-vault-call-composer = passportVaultCallComposer;

        dioxus-cli = pkgs.dioxus-cli;
      }
      // pkgs.lib.optionalAttrs pkgs.stdenv.hostPlatform.isDarwin {
        xcodegen = pkgs.xcodegen;
      };

      # The checked variants re-enable the hermetic sandbox test run that the
      # per-push package builds skip; `nix flake check` and the nightly
      # workflow build these.
      checks = {
        package = self'.packages.default.overrideAttrs (_: {
          doCheck = true;
        });
        headless = self'.packages.headless.overrideAttrs (_: {
          doCheck = true;
        });
        presentation-compact-artifacts = presentationCompactArtifacts;
        passport-vault-compact-artifacts = passportVaultCompactArtifacts;
        passport-vault-call-composer = passportVaultCallComposer;
        brand-packs =
          pkgs.runCommand "oxid-brand-packs-check"
            {
              nativeBuildInputs = [ brandCheck ];
            }
            ''
              oxid-brand-check ${../../brands}
              touch $out
            '';
      }
      // brandChecks;
      formatter = pkgs.nixfmt;
    };
}
