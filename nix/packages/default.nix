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
      ];
      linuxNativeBuildInputs = pkgs.lib.optionals pkgs.stdenv.hostPlatform.isLinux [
        pkgs.wrapGAppsHook3
      ];
    in
    {
      packages.default = pkgs.rustPlatform.buildRustPackage {
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

      packages.dioxus-cli = pkgs.dioxus-cli;

      checks.package = self'.packages.default;
      formatter = pkgs.nixfmt;
    };
}
