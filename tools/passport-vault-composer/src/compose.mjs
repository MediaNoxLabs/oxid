// SPDX-License-Identifier: Apache-2.0

import { Buffer } from "node:buffer";
import { registerHooks } from "node:module";
import path from "node:path";
import { pathToFileURL } from "node:url";

import { CompiledContract } from "@midnight-ntwrk/compact-js";
import { ChargedState, ContractState } from "@midnight-ntwrk/compact-runtime";
import { LedgerParameters, ZswapChainState } from "@midnight-ntwrk/ledger-v8";
import { createUnprovenCallTxFromInitialStates } from "@midnight-ntwrk/midnight-js-contracts";
import { setNetworkId } from "@midnight-ntwrk/midnight-js-network-id";
import { NodeZkConfigProvider } from "@midnight-ntwrk/midnight-js-node-zk-config-provider";

const COMPACT_RUNTIME_URL = import.meta.resolve("@midnight-ntwrk/compact-runtime");

const HEX_32 = /^[0-9a-f]{64}$/u;
const NETWORK_ID = /^[a-z0-9][a-z0-9-]{0,63}$/u;
const DECIMAL = /^(?:0|[1-9][0-9]*)$/u;
const MAX_U64 = (1n << 64n) - 1n;
const MAX_U128 = (1n << 128n) - 1n;
const MAX_U16 = (1n << 16n) - 1n;
const MAX_U32 = (1n << 32n) - 1n;
const MAX_FIELD_ENCODING = (1n << 256n) - 1n;
const MAX_CONTRACT_STATE_HEX = 32 * 1024 * 1024;
const MAX_ZSWAP_STATE_HEX = 4 * 1024 * 1024;
const MAX_LEDGER_PARAMETERS_HEX = 1024 * 1024;
const ZERO_32 = new Uint8Array(32);

let contractModulePromise;
let resolutionHookInstalled = false;

export class ComposerError extends Error {
  constructor(code, message) {
    super(message);
    this.name = "ComposerError";
    this.code = code;
  }
}

function invalidRequest() {
  return new ComposerError("invalid_request", "Passport Vault composer request is invalid");
}

function assertObject(value) {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    throw invalidRequest();
  }
  return value;
}

function assertExactKeys(value, keys) {
  const actual = Object.keys(value).sort();
  const expected = [...keys].sort();
  if (actual.length !== expected.length || actual.some((key, index) => key !== expected[index])) {
    throw invalidRequest();
  }
}

function parseHex32(value, allowZero = true) {
  if (typeof value !== "string" || !HEX_32.test(value)) {
    throw invalidRequest();
  }
  if (!allowZero && value === "0".repeat(64)) {
    throw invalidRequest();
  }
  return hexToBytes(value);
}

function parseBoundedHex(value, maxCharacters, nullable) {
  if (nullable && value === null) {
    return null;
  }
  if (
    typeof value !== "string" ||
    value.length === 0 ||
    value.length > maxCharacters ||
    value.length % 2 !== 0 ||
    !/^[0-9a-f]+$/u.test(value)
  ) {
    throw invalidRequest();
  }
  return hexToBytes(value);
}

function parseDecimal(value, maximum, allowZero) {
  if (typeof value !== "string" || !DECIMAL.test(value)) {
    throw invalidRequest();
  }
  const parsed = BigInt(value);
  if (parsed > maximum || (!allowZero && parsed === 0n)) {
    throw invalidRequest();
  }
  return parsed;
}

function parseOptionalPolicyValue(value) {
  if (value === null) {
    return { required: false, bytes: ZERO_32 };
  }
  return { required: true, bytes: parseHex32(value, false) };
}

function parseByteArray(value, length) {
  if (
    !Array.isArray(value) ||
    value.length !== length ||
    value.some((byte) => !Number.isInteger(byte) || byte < 0 || byte > 255)
  ) {
    throw invalidRequest();
  }
  return new Uint8Array(value);
}

function parseInteger(value, maximum) {
  if (!Number.isInteger(value) || value < 0 || BigInt(value) > maximum) {
    throw invalidRequest();
  }
  return BigInt(value);
}

function parseBoolean(value) {
  if (typeof value !== "boolean") {
    throw invalidRequest();
  }
  return value;
}

function littleEndianBigInt(bytes) {
  let value = 0n;
  for (let index = bytes.length - 1; index >= 0; index -= 1) {
    value = (value << 8n) | BigInt(bytes[index]);
  }
  if (value > MAX_FIELD_ENCODING) {
    throw invalidRequest();
  }
  return value;
}

function parseMethod(value) {
  const method = assertObject(value);
  assertExactKeys(method, ["didContractAddress", "methodId"]);
  return {
    didContractAddress: { bytes: parseByteArray(method.didContractAddress, 32) },
    methodId: parseByteArray(method.methodId, 32),
  };
}

function parseSchema(value) {
  const schema = assertObject(value);
  assertExactKeys(schema, ["packageId", "schemaId", "majorVersion", "minorVersion"]);
  return {
    packageId: parseByteArray(schema.packageId, 32),
    schemaId: parseByteArray(schema.schemaId, 32),
    majorVersion: parseInteger(schema.majorVersion, MAX_U16),
    minorVersion: parseInteger(schema.minorVersion, MAX_U16),
  };
}

function parsePoint(value) {
  const point = assertObject(value);
  assertExactKeys(point, ["xLe", "yLe"]);
  return {
    x: littleEndianBigInt(parseByteArray(point.xLe, 32)),
    y: littleEndianBigInt(parseByteArray(point.yLe, 32)),
  };
}

function parseProof(value) {
  const proof = assertObject(value);
  assertExactKeys(proof, [
    "signer",
    "createdAt",
    "challengeHash",
    "publicKey",
    "announcement",
    "responseLe",
  ]);
  return {
    signerVerificationMethodRef: parseMethod(proof.signer),
    createdAt: parseDecimal(proof.createdAt, MAX_U64, true),
    challengeHash: parseByteArray(proof.challengeHash, 32),
    publicKey: parsePoint(proof.publicKey),
    signature: {
      r: parsePoint(proof.announcement),
      s: littleEndianBigInt(parseByteArray(proof.responseLe, 32)),
    },
  };
}

function parseCredential(value) {
  const credential = assertObject(value);
  assertExactKeys(credential, [
    "version",
    "packageId",
    "schemaId",
    "majorVersion",
    "minorVersion",
    "issuer",
    "holder",
    "issuedAt",
    "hasExpiration",
    "expiresAt",
    "firstNameCommitment",
    "lastNameCommitment",
    "dateOfBirthCommitment",
    "documentNumberCommitment",
    "issuingStateCommitment",
    "claimRoot",
  ]);
  return {
    version: parseInteger(credential.version, MAX_U16),
    schema: parseSchema({
      packageId: credential.packageId,
      schemaId: credential.schemaId,
      majorVersion: credential.majorVersion,
      minorVersion: credential.minorVersion,
    }),
    issuerVerificationMethodRef: parseMethod(credential.issuer),
    holderBinding: { holderVerificationMethodRef: parseMethod(credential.holder) },
    statusBinding: {},
    issuedAt: parseDecimal(credential.issuedAt, MAX_U64, true),
    hasExpiration: parseBoolean(credential.hasExpiration),
    expiresAt: parseDecimal(credential.expiresAt, MAX_U64, true),
    claims: {},
    claimCommitments: {
      firstNameCommitment: parseByteArray(credential.firstNameCommitment, 32),
      lastNameCommitment: parseByteArray(credential.lastNameCommitment, 32),
      dateOfBirthCommitment: parseByteArray(credential.dateOfBirthCommitment, 32),
      documentNumberCommitment: parseByteArray(credential.documentNumberCommitment, 32),
      issuingStateCommitment: parseByteArray(credential.issuingStateCommitment, 32),
    },
    claimRoot: parseByteArray(credential.claimRoot, 32),
  };
}

function parseDisclosures(value) {
  const disclosures = assertObject(value);
  assertExactKeys(disclosures, [
    "revealFirstName",
    "firstNameValuePadded",
    "firstNameOpening",
    "revealLastName",
    "lastNameValuePadded",
    "lastNameOpening",
    "proveAgeOverThreshold",
    "ageThresholdYears",
    "revealDocumentNumber",
    "documentNumberValue",
    "documentNumberOpening",
    "revealIssuingState",
    "issuingStateValue",
    "issuingStateOpening",
  ]);
  return {
    revealFirstName: parseBoolean(disclosures.revealFirstName),
    firstNameValuePadded: parseByteArray(disclosures.firstNameValuePadded, 64),
    firstNameOpening: parseByteArray(disclosures.firstNameOpening, 32),
    revealLastName: parseBoolean(disclosures.revealLastName),
    lastNameValuePadded: parseByteArray(disclosures.lastNameValuePadded, 64),
    lastNameOpening: parseByteArray(disclosures.lastNameOpening, 32),
    proveAgeOverThreshold: parseBoolean(disclosures.proveAgeOverThreshold),
    ageThresholdYears: parseInteger(disclosures.ageThresholdYears, 120n),
    revealDocumentNumber: parseBoolean(disclosures.revealDocumentNumber),
    documentNumberValue: parseByteArray(disclosures.documentNumberValue, 32),
    documentNumberOpening: parseByteArray(disclosures.documentNumberOpening, 32),
    revealIssuingState: parseBoolean(disclosures.revealIssuingState),
    issuingStateValue: parseByteArray(disclosures.issuingStateValue, 32),
    issuingStateOpening: parseByteArray(disclosures.issuingStateOpening, 32),
  };
}

function parsePresentation(value) {
  const presentation = assertObject(value);
  assertExactKeys(presentation, [
    "version",
    "packageId",
    "schemaId",
    "majorVersion",
    "minorVersion",
    "credentialClaimRoot",
    "issuer",
    "holder",
    "disclosures",
  ]);
  return {
    version: parseInteger(presentation.version, MAX_U16),
    schema: parseSchema({
      packageId: presentation.packageId,
      schemaId: presentation.schemaId,
      majorVersion: presentation.majorVersion,
      minorVersion: presentation.minorVersion,
    }),
    credentialClaimRoot: parseByteArray(presentation.credentialClaimRoot, 32),
    issuerVerificationMethodRef: parseMethod(presentation.issuer),
    holderBinding: { holderVerificationMethodRef: parseMethod(presentation.holder) },
    disclosed: parseDisclosures(presentation.disclosures),
  };
}

function parseClaimMaterial(value) {
  const material = assertObject(value);
  assertExactKeys(material, [
    "credential",
    "credentialProof",
    "presentation",
    "presentationProof",
    "currentDay",
    "witness",
  ]);
  const witness = assertObject(material.witness);
  assertExactKeys(witness, ["holderDateOfBirthDays", "holderDateOfBirthOpening"]);
  return {
    credential: parseCredential(material.credential),
    credentialProof: parseProof(material.credentialProof),
    presentation: parsePresentation(material.presentation),
    presentationProof: parseProof(material.presentationProof),
    currentDay: parseInteger(material.currentDay, MAX_U32),
    privateState: {
      holderDateOfBirthDays: parseInteger(witness.holderDateOfBirthDays, MAX_U32),
      holderDateOfBirthOpening: parseByteArray(witness.holderDateOfBirthOpening, 32),
    },
  };
}

function parseOperation(value) {
  const operation = assertObject(value);
  if (typeof operation.kind !== "string") {
    throw invalidRequest();
  }
  switch (operation.kind) {
    case "create_lock": {
      assertExactKeys(operation, [
        "kind",
        "minimumAgeYears",
        "requiredIssuingStateHex",
        "requiredDocumentNumberHex",
        "maximumClaimAmount",
        "verifierChallengeHashHex",
        "initialAmount",
      ]);
      if (
        !Number.isInteger(operation.minimumAgeYears) ||
        operation.minimumAgeYears < 0 ||
        operation.minimumAgeYears > 120
      ) {
        throw invalidRequest();
      }
      const issuingState = parseOptionalPolicyValue(operation.requiredIssuingStateHex);
      const documentNumber = parseOptionalPolicyValue(operation.requiredDocumentNumberHex);
      return {
        kind: operation.kind,
        circuitId: "createLock",
        args: [
          BigInt(operation.minimumAgeYears),
          issuingState.required,
          issuingState.bytes,
          documentNumber.required,
          documentNumber.bytes,
          parseDecimal(operation.maximumClaimAmount, MAX_U128, false),
          parseHex32(operation.verifierChallengeHashHex, false),
          parseDecimal(operation.initialAmount, MAX_U128, true),
        ],
      };
    }
    case "deposit_to_lock":
      assertExactKeys(operation, ["kind", "lockId", "amount"]);
      return {
        kind: operation.kind,
        circuitId: "depositToLock",
        args: [
          parseDecimal(operation.lockId, MAX_U64, true),
          parseDecimal(operation.amount, MAX_U128, false),
        ],
      };
    case "withdraw_from_lock":
      assertExactKeys(operation, ["kind", "lockId", "amount", "recipientAddressHex"]);
      return {
        kind: operation.kind,
        circuitId: "withdrawFromLock",
        args: [
          parseDecimal(operation.lockId, MAX_U64, true),
          parseDecimal(operation.amount, MAX_U128, false),
          { bytes: parseHex32(operation.recipientAddressHex) },
        ],
      };
    case "claim_from_lock": {
      assertExactKeys(operation, [
        "kind",
        "lockId",
        "amount",
        "recipientAddressHex",
        "material",
      ]);
      const material = parseClaimMaterial(operation.material);
      return {
        kind: operation.kind,
        circuitId: "claimFromLock",
        args: [
          parseDecimal(operation.lockId, MAX_U64, true),
          material.credential,
          material.credentialProof,
          material.presentation,
          material.presentationProof,
          material.currentDay,
          parseDecimal(operation.amount, MAX_U128, false),
          { bytes: parseHex32(operation.recipientAddressHex) },
        ],
        privateState: material.privateState,
      };
    }
    case "set_trusted_issuer":
      throw new ComposerError(
        "administrative_circuit_forbidden",
        "Passport Vault administration is unavailable to wallet composition",
      );
    default:
      throw new ComposerError("unsupported_operation", "Passport Vault operation is unsupported");
  }
}

function parseRequest(value) {
  const request = assertObject(value);
  assertExactKeys(request, ["schemaVersion", "operation", "chain", "wallet"]);
  if (request.schemaVersion !== 1) {
    throw invalidRequest();
  }

  const chain = assertObject(request.chain);
  assertExactKeys(chain, [
    "contractStateHex",
    "contractAddressHex",
    "zswapChainStateHex",
    "ledgerParametersHex",
    "networkId",
  ]);
  if (typeof chain.networkId !== "string" || !NETWORK_ID.test(chain.networkId)) {
    throw invalidRequest();
  }

  const wallet = assertObject(request.wallet);
  assertExactKeys(wallet, ["coinPublicKeyHex", "encryptionPublicKeyHex"]);

  parseHex32(chain.contractAddressHex);
  parseHex32(wallet.coinPublicKeyHex);
  parseHex32(wallet.encryptionPublicKeyHex);

  return {
    operation: parseOperation(request.operation),
    contractState: parseBoundedHex(
      chain.contractStateHex,
      MAX_CONTRACT_STATE_HEX,
      false,
    ),
    contractAddressHex: chain.contractAddressHex,
    zswapChainState: parseBoundedHex(
      chain.zswapChainStateHex,
      MAX_ZSWAP_STATE_HEX,
      true,
    ),
    ledgerParameters: parseBoundedHex(
      chain.ledgerParametersHex,
      MAX_LEDGER_PARAMETERS_HEX,
      true,
    ),
    networkId: chain.networkId,
    coinPublicKeyHex: wallet.coinPublicKeyHex,
    encryptionPublicKeyHex: wallet.encryptionPublicKeyHex,
  };
}

function hexToBytes(value) {
  return new Uint8Array(Buffer.from(value, "hex"));
}

function bytesToHex(value) {
  return Buffer.from(value).toString("hex");
}

async function loadContractModule() {
  const root = artifactRoot();
  if (!resolutionHookInstalled) {
    registerHooks({
      resolve(specifier, context, nextResolve) {
        if (specifier === "@midnight-ntwrk/compact-runtime") {
          return { url: COMPACT_RUNTIME_URL, shortCircuit: true };
        }
        return nextResolve(specifier, context);
      },
    });
    resolutionHookInstalled = true;
  }
  const generatedContract = path.join(root, "contract", "index.js");
  contractModulePromise ??= import(pathToFileURL(generatedContract).href);
  return contractModulePromise;
}

function artifactRoot() {
  const root = process.env.OXID_PASSPORT_VAULT_ARTIFACTS_DIR;
  if (
    typeof root !== "string" ||
    !path.isAbsolute(root) ||
    path.normalize(root) !== root
  ) {
    throw new ComposerError("unavailable", "Passport Vault composer is unavailable");
  }
  return root;
}

async function executePassportVaultCall(value) {
  const request = parseRequest(value);
  const generated = await loadContractModule();
  const artifacts = artifactRoot();
  const witnesses = {
    holderDateOfBirthDays({ privateState }) {
      if (typeof privateState?.holderDateOfBirthDays !== "bigint") {
        throw invalidRequest();
      }
      return [privateState, privateState.holderDateOfBirthDays];
    },
    holderDateOfBirthOpening({ privateState }) {
      if (!(privateState?.holderDateOfBirthOpening instanceof Uint8Array)) {
        throw invalidRequest();
      }
      return [privateState, privateState.holderDateOfBirthOpening];
    },
  };
  const compiledContract = CompiledContract.make("passport-vault", generated.Contract).pipe(
    CompiledContract.withWitnesses(witnesses),
    CompiledContract.withCompiledFileAssets(artifacts),
  );

  setNetworkId(request.networkId);
  const contractState = ContractState.deserialize(request.contractState);
  const zswapChainState = request.zswapChainState
    ? ZswapChainState.deserialize(request.zswapChainState)
    : new ZswapChainState();
  const ledgerParameters = request.ledgerParameters
    ? LedgerParameters.deserialize(request.ledgerParameters)
    : LedgerParameters.initialParameters();
  const zkConfigProvider = new NodeZkConfigProvider(artifacts);

  let call;
  try {
    call = await createUnprovenCallTxFromInitialStates(
      zkConfigProvider,
      {
        compiledContract,
        circuitId: request.operation.circuitId,
        contractAddress: request.contractAddressHex,
        args: request.operation.args,
        coinPublicKey: request.coinPublicKeyHex,
        initialContractState: contractState,
        initialZswapChainState: zswapChainState,
        ledgerParameters,
        initialPrivateState: request.operation.privateState ?? {},
      },
      request.encryptionPublicKeyHex,
    );
  } catch (error) {
    if (error instanceof ComposerError) {
      throw error;
    }
    throw new ComposerError("composition_failed", "Passport Vault call composition failed");
  }

  return { call, contractState, request };
}

function responseFor(call, request) {
  const serialized = call.private.unprovenTx.serialize();
  return {
    schemaVersion: 1,
    ok: true,
    operationKind: request.operation.kind,
    circuitId: request.operation.circuitId,
    unprovenTransactionHex: bytesToHex(serialized),
    unprovenTransactionBytes: serialized.length,
  };
}

export async function composePassportVaultCall(value) {
  const { call, request } = await executePassportVaultCall(value);
  return responseFor(call, request);
}

/** Test-only state chaining for circuit conformance. Never use this state as
 * authenticated chain authority or expose it through an incoming adapter. */
export async function composePassportVaultCallForConformance(value) {
  const { call, contractState, request } = await executePassportVaultCall(value);
  contractState.data = new ChargedState(call.public.nextContractState);
  return {
    ...responseFor(call, request),
    nextContractStateHex: bytesToHex(contractState.serialize()),
  };
}
