{
  lib,
  stdenvNoCC,
  fetchurl,
  jq,
}:

stdenvNoCC.mkDerivation {
  pname = "oxid-midnight-did-compact-artifacts";
  version = "0.5.0";

  src = fetchurl {
    url = "https://github.com/midnightntwrk/midnight-did/releases/download/v0.5.0/midnight-did-zk-artifacts-0.5.0.tar.gz";
    hash = "sha256-pMLQDv2daVbLEUjZFrgKkfKwc3i8U+DVMEnViWN3vtU=";
  };

  nativeBuildInputs = [ jq ];
  sourceRoot = ".";
  dontBuild = true;

  installPhase = ''
    runHook preInstall
    mkdir -p "$out"
    cp -R keys zkir manifest.json "$out/"
    runHook postInstall
  '';

  doInstallCheck = true;
  installCheckPhase = ''
    test "$(jq -r .schema "$out/manifest.json")" = midnight-did-zk-artifacts
    test "$(jq -r .version "$out/manifest.json")" = 0.5.0
    test "$(jq -r .gitSha "$out/manifest.json")" = a14267cec3c1ab7e00bb0f058a54267d913a321b
    for circuit in setVerificationMethod setSchnorrJubjubVerificationMethod setVerificationMethodRelation; do
      test -s "$out/keys/$circuit.prover"
      test -s "$out/keys/$circuit.verifier"
      test -s "$out/zkir/$circuit.bzkir"
    done
  '';

  meta = {
    description = "Authenticated Midnight DID 0.5.0 Compact artifacts";
    homepage = "https://github.com/midnightntwrk/midnight-did/releases/tag/v0.5.0";
    license = lib.licenses.asl20;
  };
}
