{
  compactMidnight,
  compactToolchain,
  coreutils,
  jq,
  lib,
  midnightCircuitParams,
  midnightVcSource,
  passportVaultSource,
  stdenvNoCC,
}:

let
  vaultRevision = "e4a92a6be2cc6dc34f68261f10c19c9312043807";
  vcRevision = "39b1354212620b396e914b29603e6a38f2656546";
  compilerRevision = "05b237a5e51f9c22853b424e7d4236dfa9384c24";
  vaultSource = "${passportVaultSource}/packages/contracts/vault/src/passport-vault.compact";
in
stdenvNoCC.mkDerivation {
  pname = "oxid-passport-vault-compact-artifacts";
  version = "0.1.0";

  src = passportVaultSource;
  nativeBuildInputs = [
    compactMidnight
    compactToolchain
    coreutils
    jq
  ];

  buildPhase = ''
    runHook preBuild

    export HOME="$TMPDIR/home"
    export COMPACT_DIRECTORY=${compactToolchain}
    mkdir -p "$HOME/.cache/midnight/zk-params" source/vendored generated
    cp -R ${midnightCircuitParams}/. "$HOME/.cache/midnight/zk-params/"
    cp ${vaultSource} source/passport-vault.compact

    cp ${midnightVcSource}/packages/core/primitives/credentials/src/credentials.compact \
      source/vendored/credentials.compact
    cp -R ${midnightVcSource}/packages/core/primitives/credentials/src/credentials \
      source/vendored/credentials
    cp ${midnightVcSource}/packages/prototypes/credential-families/digital-passport/src/digital-passport-credential.compact \
      source/vendored/digital-passport-credential.compact
    cp -R ${midnightVcSource}/packages/prototypes/credential-families/digital-passport/src/digital-passport-credential \
      source/vendored/digital-passport-credential
    substituteInPlace source/vendored/digital-passport-credential.compact \
      --replace-fail \
        'include "../../../../../packages/core/primitives/credentials/src/credentials";' \
        'include "./credentials";'

    test "$(compact --version)" = "compact 0.5.1"
    compiler="$(readlink ${compactToolchain}/bin/compactc)"
    compiler_directory="$(dirname "$compiler")"
    test "$("$compiler" --version)" = "0.30.0"

    "$compiler" --skip-zk source/passport-vault.compact generated
    mkdir -p generated/keys
    for circuit in \
      setTrustedIssuer \
      createLock \
      depositToLock \
      claimFromLock \
      withdrawFromLock
    do
      "$compiler_directory/zkir" mock-compile "generated/zkir/$circuit.zkir"
      "$compiler_directory/zkir" compile -v \
        "generated/zkir/$circuit.zkir" \
        "generated/keys/$circuit.prover" \
        "generated/keys/$circuit.verifier" \
        2>&1 | tee "$TMPDIR/$circuit-build.log"
    done

    grep -q 'k=13, rows=5416' "$TMPDIR/setTrustedIssuer-build.log"
    grep -q 'k=11, rows=1823' "$TMPDIR/createLock-build.log"
    grep -q 'k=10, rows=834' "$TMPDIR/depositToLock-build.log"
    grep -q 'k=17, rows=124785' "$TMPDIR/claimFromLock-build.log"
    grep -q 'k=11, rows=1212' "$TMPDIR/withdrawFromLock-build.log"
    jq -e '
      .["compiler-version"] == "0.30.0" and
      .["language-version"] == "0.22.0" and
      .["runtime-version"] == "0.15.0" and
      (.circuits as $circuits |
        all(["setTrustedIssuer", "createLock", "depositToLock", "claimFromLock", "withdrawFromLock"][];
          . as $name |
          any($circuits[]; .name == $name and .pure == false and .proof == true)))
    ' generated/compiler/contract-info.json >/dev/null

    for circuit in \
      setTrustedIssuer \
      createLock \
      depositToLock \
      claimFromLock \
      withdrawFromLock
    do
      for extension in prover verifier; do
        test -s "generated/keys/$circuit.$extension"
      done
      for extension in bzkir zkir; do
        test -s "generated/zkir/$circuit.$extension"
      done
    done
    for relative_path in \
      compiler/contract-info.json \
      contract/index.d.ts \
      contract/index.js \
      contract/index.js.map
    do
      test -s "generated/$relative_path"
    done

    runHook postBuild
  '';

  installPhase = ''
    runHook preInstall

    mkdir -p "$out/artifacts/params" "$out/source"
    cp -R generated/. "$out/artifacts/"
    cp -R source/. "$out/source/"
    for exponent in 10 11 13 17; do
      cp "${midnightCircuitParams}/bls_midnight_2p$exponent" \
        "$out/artifacts/params/bls_midnight_2p$exponent"
    done

    entries="$TMPDIR/artifacts.ndjson"
    : > "$entries"
    while IFS= read -r relative_path; do
      bytes="$(wc -c < "$out/artifacts/$relative_path" | tr -d ' ')"
      digest="$(sha256sum "$out/artifacts/$relative_path" | cut -d ' ' -f 1)"
      jq -n \
        --arg path "$relative_path" \
        --arg sha256 "$digest" \
        --argjson bytes "$bytes" \
        '{ path: $path, bytes: $bytes, sha256: $sha256 }' >> "$entries"
    done < <(find "$out/artifacts" -type f -print \
      | sed "s|^$out/artifacts/||" \
      | LC_ALL=C sort)

    contract_digest="$(sha256sum ${vaultSource} | cut -d ' ' -f 1)"
    vault_lock_digest="$(sha256sum package-lock.json | cut -d ' ' -f 1)"
    vc_lock_digest="$(sha256sum ${midnightVcSource}/pnpm-lock.yaml | cut -d ' ' -f 1)"
    parameter_10_digest="$(sha256sum ${midnightCircuitParams}/bls_midnight_2p10 | cut -d ' ' -f 1)"
    parameter_11_digest="$(sha256sum ${midnightCircuitParams}/bls_midnight_2p11 | cut -d ' ' -f 1)"
    parameter_13_digest="$(sha256sum ${midnightCircuitParams}/bls_midnight_2p13 | cut -d ' ' -f 1)"
    parameter_17_digest="$(sha256sum ${midnightCircuitParams}/bls_midnight_2p17 | cut -d ' ' -f 1)"
    jq -s \
      --arg system "${stdenvNoCC.hostPlatform.system}" \
      --arg vaultRevision "${vaultRevision}" \
      --arg vcRevision "${vcRevision}" \
      --arg compilerRevision "${compilerRevision}" \
      --arg contractSha256 "$contract_digest" \
      --arg vaultLockSha256 "$vault_lock_digest" \
      --arg vcLockSha256 "$vc_lock_digest" \
      --arg parameter10Sha256 "$parameter_10_digest" \
      --arg parameter11Sha256 "$parameter_11_digest" \
      --arg parameter13Sha256 "$parameter_13_digest" \
      --arg parameter17Sha256 "$parameter_17_digest" \
      '{
        schemaVersion: 1,
        artifactSet: "oxid-passport-vault-v1",
        system: $system,
        source: {
          repository: "https://github.com/midnightntwrk/midnight-identity-solution-examples",
          revision: $vaultRevision,
          contract: "packages/contracts/vault/src/passport-vault.compact",
          contractSha256: $contractSha256,
          lockSha256: $vaultLockSha256,
          credentialRepository: "https://github.com/midnightntwrk/midnight-verifiable-credentials",
          credentialRevision: $vcRevision,
          credentialLockSha256: $vcLockSha256,
          license: "Apache-2.0"
        },
        toolchain: {
          compactCliVersion: "0.5.1",
          compilerVersion: "0.30.0",
          compilerLanguageVersion: "0.22.0",
          generatedRuntimeVersion: "0.15.0",
          sourceRepository: "https://github.com/midnightntwrk/midnight-did",
          sourceRevision: $compilerRevision,
          circuitParameters: [
            { name: "bls_midnight_2p10", sha256: $parameter10Sha256 },
            { name: "bls_midnight_2p11", sha256: $parameter11Sha256 },
            { name: "bls_midnight_2p13", sha256: $parameter13Sha256 },
            { name: "bls_midnight_2p17", sha256: $parameter17Sha256 }
          ]
        },
        circuits: [
          { id: "setTrustedIssuer", k: 13, rows: 5416 },
          { id: "createLock", k: 11, rows: 1823 },
          { id: "depositToLock", k: 10, rows: 834 },
          { id: "claimFromLock", k: 17, rows: 124785 },
          { id: "withdrawFromLock", k: 11, rows: 1212 }
        ],
        artifacts: .
      }' "$entries" > "$out/manifest.json"

    runHook postInstall
  '';

  dontFixup = true;
  strictDeps = true;

  meta = {
    description = "Reproducible Passport Vault Compact contract artifacts";
    homepage = "https://github.com/MediaNoxLabs/oxid";
    license = lib.licenses.asl20;
    platforms = [
      "x86_64-linux"
      "aarch64-darwin"
    ];
  };
}
