{
  compactMidnight,
  compactToolchain,
  coreutils,
  jq,
  lib,
  midnightCircuitParams,
  midnightVcSource,
  stdenvNoCC,
}:

let
  upstreamRevision = "39b1354212620b396e914b29603e6a38f2656546";
  compilerRevision = "05b237a5e51f9c22853b424e7d4236dfa9384c24";
  presentationSource = ../../contracts/presentation/digital-passport-presentation.compact;
in
stdenvNoCC.mkDerivation {
  pname = "oxid-digital-passport-presentation-artifacts";
  version = "0.1.0";

  src = midnightVcSource;
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
    mkdir -p "$HOME/.cache/midnight/zk-params" oxid generated
    cp -R ${midnightCircuitParams}/. "$HOME/.cache/midnight/zk-params/"
    cp ${presentationSource} oxid/digital-passport-presentation.compact

    test "$(compact --version)" = "compact 0.5.1"
    compiler="$(readlink ${compactToolchain}/bin/compactc)"
    compiler_directory="$(dirname "$compiler")"
    test "$("$compiler" --version)" = "0.30.0"

    "$compiler" --skip-zk \
      oxid/digital-passport-presentation.compact \
      generated
    "$compiler_directory/zkir" mock-compile \
      generated/zkir/proveDigitalPassportPresentation.zkir
    mkdir -p generated/keys
    "$compiler_directory/zkir" compile -v \
      generated/zkir/proveDigitalPassportPresentation.zkir \
      generated/keys/proveDigitalPassportPresentation.prover \
      generated/keys/proveDigitalPassportPresentation.verifier \
      2>&1 | tee "$TMPDIR/compact-build.log"

    jq -e '
      .["compiler-version"] == "0.30.0" and
      .["language-version"] == "0.22.0" and
      .["runtime-version"] == "0.15.0" and
      any(.circuits[];
        .name == "proveDigitalPassportPresentation" and
        .pure == false and
        .proof == true)
    ' generated/compiler/contract-info.json >/dev/null
    grep -q 'k=18, rows=156301' "$TMPDIR/compact-build.log"

    for relative_path in \
      compiler/contract-info.json \
      contract/index.d.ts \
      contract/index.js \
      contract/index.js.map \
      keys/proveDigitalPassportPresentation.prover \
      keys/proveDigitalPassportPresentation.verifier \
      zkir/proveDigitalPassportPresentation.bzkir \
      zkir/proveDigitalPassportPresentation.zkir
    do
      test -s "generated/$relative_path"
    done

    runHook postBuild
  '';

  installPhase = ''
    runHook preInstall

    mkdir -p "$out/artifacts"
    cp -R generated/. "$out/artifacts/"

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

    contract_digest="$(sha256sum ${presentationSource} | cut -d ' ' -f 1)"
    upstream_lock_digest="$(sha256sum pnpm-lock.yaml | cut -d ' ' -f 1)"
    parameter_digest="$(sha256sum ${midnightCircuitParams}/bls_midnight_2p18 | cut -d ' ' -f 1)"
    jq -s \
      --arg system "${stdenvNoCC.hostPlatform.system}" \
      --arg upstreamRevision "${upstreamRevision}" \
      --arg compilerRevision "${compilerRevision}" \
      --arg contractSha256 "$contract_digest" \
      --arg upstreamLockSha256 "$upstream_lock_digest" \
      --arg parameterSha256 "$parameter_digest" \
      '{
        schemaVersion: 1,
        artifactSet: "oxid-digital-passport-presentation-v1",
        system: $system,
        source: {
          oxidContract: "contracts/presentation/digital-passport-presentation.compact",
          oxidContractSha256: $contractSha256,
          upstreamRepository: "https://github.com/midnightntwrk/midnight-verifiable-credentials",
          upstreamRevision: $upstreamRevision,
          upstreamLicense: "Apache-2.0",
          upstreamLockSha256: $upstreamLockSha256
        },
        toolchain: {
          compactCliVersion: "0.5.1",
          compilerVersion: "0.30.0",
          compilerLanguageVersion: "0.22.0",
          generatedRuntimeVersion: "0.15.0",
          toolchainSourceRepository: "https://github.com/midnightntwrk/midnight-did",
          toolchainSourceRevision: $compilerRevision,
          circuitParameter: "bls_midnight_2p18",
          circuitParameterSha256: $parameterSha256
        },
        circuit: {
          id: "proveDigitalPassportPresentation",
          k: 18,
          rows: 156301,
          publicStatementDomain: "oxid:midnight-compact-vp:v1"
        },
        artifacts: .
      }' "$entries" > "$out/manifest.json"

    runHook postInstall
  '';

  dontFixup = true;
  strictDeps = true;

  meta = {
    description = "Reproducible Oxid Digital Passport Compact presentation artifacts";
    homepage = "https://github.com/MediaNoxLabs/oxid";
    license = lib.licenses.asl20;
    platforms = [
      "x86_64-linux"
      "aarch64-darwin"
    ];
  };
}
