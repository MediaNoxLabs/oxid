{ ... }:

{
  perSystem =
    { pkgs, self', ... }:
    let
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

          cargoLock.lockFile = ../../Cargo.lock;
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

          cargoLock.lockFile = ../../Cargo.lock;
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

        dioxus-cli = pkgs.dioxus-cli;
      }
      // pkgs.lib.optionalAttrs pkgs.stdenv.hostPlatform.isDarwin {
        xcodegen = pkgs.xcodegen;
      };

      checks.package = self'.packages.default;
      checks.headless = self'.packages.headless;
      formatter = pkgs.nixfmt;
    };
}
