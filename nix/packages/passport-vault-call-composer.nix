{
  buildNpmPackage,
  coreutils,
  jq,
  lib,
  makeWrapper,
  nodejs_24,
  passportVaultCompactArtifacts,
  vaultContractStateFixture,
}:

buildNpmPackage {
  pname = "oxid-passport-vault-call-composer";
  version = "0.1.0";

  src = ../../tools/passport-vault-composer;
  nodejs = nodejs_24;
  npmDepsHash = "sha256-yXxplbc3y8XnINE/kIheAcjyyCxT1vLcyUxfRk+77r4=";

  nativeBuildInputs = [ makeWrapper ];
  nativeInstallCheckInputs = [
    coreutils
    jq
  ];
  dontNpmBuild = true;

  installPhase = ''
    runHook preInstall

    runtime="$out/libexec/oxid-passport-vault-call-composer"
    mkdir -p "$out/bin" "$runtime"
    cp -R src node_modules package.json "$runtime/"
    makeWrapper ${lib.getExe nodejs_24} "$out/bin/oxid-passport-vault-call-composer" \
      --add-flags "$runtime/src/main.mjs" \
      --set OXID_PASSPORT_VAULT_ARTIFACTS_DIR ${passportVaultCompactArtifacts}/artifacts \
      --unset NODE_OPTIONS \
      --unset NODE_PATH

    runHook postInstall
  '';

  doInstallCheck = true;
  installCheckPhase = ''
    runHook preInstallCheck

    contract_state="$(${coreutils}/bin/tr -d '\r\n' < ${vaultContractStateFixture})"
    OXID_PASSPORT_VAULT_ARTIFACTS_DIR=${passportVaultCompactArtifacts}/artifacts \
      OXID_PASSPORT_VAULT_CONTRACT_STATE_FIXTURE="$contract_state" \
      npm test

    ${lib.getExe jq} -n \
      --arg contractStateHex "$contract_state" \
      '{
        schemaVersion: 1,
        operation: {
          kind: "create_lock",
          minimumAgeYears: 18,
          requiredIssuingStateHex: null,
          requiredDocumentNumberHex: null,
          maximumClaimAmount: "40",
          verifierChallengeHashHex: ("01" * 32),
          initialAmount: "0"
        },
        chain: {
          contractStateHex: $contractStateHex,
          contractAddressHex: ("00" * 32),
          zswapChainStateHex: null,
          ledgerParametersHex: null,
          networkId: "undeployed"
        },
        wallet: {
          coinPublicKeyHex: "1bd4f827be97ff013c4a702e4b08f30ec378728a54670cf7cc92cb9b1a14eff6",
          encryptionPublicKeyHex: "b62e630a030171b5e11af2487f0103e650cc703f284d0a478b2a3abdf9715b70"
        }
      }' > "$TMPDIR/request.json"

    "$out/bin/oxid-passport-vault-call-composer" \
      < "$TMPDIR/request.json" > "$TMPDIR/response.json"
    ${lib.getExe jq} -e '
      .schemaVersion == 1 and
      .ok == true and
      .operationKind == "create_lock" and
      .circuitId == "createLock" and
      .unprovenTransactionBytes > 100 and
      (.unprovenTransactionHex | test("^[0-9a-f]+$") and length % 2 == 0)
    ' "$TMPDIR/response.json" >/dev/null

    runHook postInstallCheck
  '';

  strictDeps = true;

  meta = {
    description = "Bounded generated-Compact Passport Vault call composer";
    homepage = "https://github.com/MediaNoxLabs/oxid";
    license = lib.licenses.asl20;
    mainProgram = "oxid-passport-vault-call-composer";
    platforms = [
      "x86_64-linux"
      "aarch64-darwin"
    ];
  };
}
