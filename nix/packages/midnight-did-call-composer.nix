{
  buildNpmPackage,
  coreutils,
  lib,
  makeWrapper,
  midnightDidCompactArtifacts,
  midnightDidNpmArtifacts,
  nodejs_24,
}:

buildNpmPackage {
  pname = "oxid-midnight-did-call-composer";
  version = "0.1.0";

  src = ../../tools/did-call-composer;
  nodejs = nodejs_24;
  npmDepsHash = "sha256-kalOt5NiXIv9JVScvXy1cImGwgJYMxfLynSD+As0oNY=";

  nativeBuildInputs = [ makeWrapper ];
  nativeInstallCheckInputs = [ coreutils ];
  dontNpmBuild = true;

  installPhase = ''
    runHook preInstall

    runtime="$out/libexec/oxid-midnight-did-call-composer"
    artifacts="$out/share/oxid-midnight-did-call-artifacts"
    unpacked="$TMPDIR/midnight-did-contract"
    mkdir -p "$out/bin" "$runtime" "$artifacts" "$unpacked"
    cp -R src node_modules package.json "$runtime/"

    tar -xzf \
      ${midnightDidNpmArtifacts}/midnight-ntwrk-midnight-did-contract-0.4.0.tgz \
      -C "$unpacked"
    cp -R "$unpacked/package/dist/managed/did/contract" "$artifacts/contract"
    cp -R ${midnightDidCompactArtifacts}/keys "$artifacts/keys"
    cp -R ${midnightDidCompactArtifacts}/zkir "$artifacts/zkir"
    cp ${midnightDidCompactArtifacts}/manifest.json "$artifacts/manifest.json"

    makeWrapper ${lib.getExe nodejs_24} "$out/bin/oxid-midnight-did-call-composer" \
      --add-flags "$runtime/src/main.mjs" \
      --set OXID_MIDNIGHT_DID_CALL_ARTIFACTS_DIR "$artifacts" \
      --unset NODE_OPTIONS \
      --unset NODE_PATH

    runHook postInstall
  '';

  doInstallCheck = true;
  installCheckPhase = ''
    runHook preInstallCheck

    test -s "$out/share/oxid-midnight-did-call-artifacts/contract/index.js"
    test -s "$out/share/oxid-midnight-did-call-artifacts/keys/setVerificationMethod.prover"
    test -s "$out/share/oxid-midnight-did-call-artifacts/keys/setSchnorrJubjubVerificationMethod.prover"
    test -s "$out/share/oxid-midnight-did-call-artifacts/keys/setVerificationMethodRelation.prover"
    OXID_MIDNIGHT_DID_CALL_ARTIFACTS_DIR="$out/share/oxid-midnight-did-call-artifacts" \
      npm test

    runHook postInstallCheck
  '';

  strictDeps = true;

  meta = {
    description = "Bounded generated-Compact Midnight DID call composer";
    homepage = "https://github.com/MediaNoxLabs/oxid";
    license = lib.licenses.asl20;
    mainProgram = "oxid-midnight-did-call-composer";
    platforms = [
      "x86_64-linux"
      "aarch64-darwin"
    ];
  };
}
