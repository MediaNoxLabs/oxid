{
  lib,
  stdenvNoCC,
  fetchurl,
  jq,
}:

stdenvNoCC.mkDerivation {
  pname = "oxid-midnight-did-compact-artifacts";
  version = "0.4.0";

  src = fetchurl {
    url = "https://github.com/midnightntwrk/midnight-did/releases/download/v0.4.0/midnight-did-zk-artifacts-0.4.0.tar.gz";
    hash = "sha256-K5qTtp064ynCUvV5F0Y1KMxGql0Q8h4ysSExVeGIPow=";
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
    test "$(jq -r .version "$out/manifest.json")" = 0.4.0
    test "$(jq -r .gitSha "$out/manifest.json")" = cf00aacb3e1bb300e87bc4dd11ec0897fab6e233
    for circuit in setVerificationMethod setSchnorrJubjubVerificationMethod setVerificationMethodRelation; do
      test -s "$out/keys/$circuit.prover"
      test -s "$out/keys/$circuit.verifier"
      test -s "$out/zkir/$circuit.bzkir"
    done
  '';

  meta = {
    description = "Authenticated Midnight DID 0.4.0 Compact artifacts";
    homepage = "https://github.com/midnightntwrk/midnight-did/releases/tag/v0.4.0";
    license = lib.licenses.asl20;
  };
}
