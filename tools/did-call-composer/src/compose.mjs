// SPDX-License-Identifier: Apache-2.0

import { Buffer } from "node:buffer";
import { registerHooks } from "node:module";
import path from "node:path";
import { pathToFileURL } from "node:url";

import { CompiledContract } from "@midnight-ntwrk/compact-js";
import { ContractState } from "@midnight-ntwrk/compact-runtime";
import { LedgerParameters, ZswapChainState } from "@midnight-ntwrk/ledger-v8";
import { createUnprovenCallTxFromInitialStates } from "@midnight-ntwrk/midnight-js-contracts";
import { setNetworkId } from "@midnight-ntwrk/midnight-js-network-id";
import { NodeZkConfigProvider } from "@midnight-ntwrk/midnight-js-node-zk-config-provider";

const COMPACT_RUNTIME_URL = import.meta.resolve("@midnight-ntwrk/compact-runtime");
const HEX_32 = /^[0-9a-f]{64}$/u;
const NETWORK_ID = /^[a-z0-9][a-z0-9-]{0,63}$/u;
const METHOD_ID = /^#[A-Za-z0-9._~-]{1,64}$/u;
const MAX_CONTRACT_STATE_HEX = 32 * 1024 * 1024;
const MAX_ZSWAP_STATE_HEX = 4 * 1024 * 1024;
const MAX_LEDGER_PARAMETERS_HEX = 1024 * 1024;

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
  return new ComposerError("invalid_request", "DID composer request is invalid");
}

function object(value) {
  if (value === null || typeof value !== "object" || Array.isArray(value)) throw invalidRequest();
  return value;
}

function exact(value, keys) {
  const actual = Object.keys(value).sort();
  const expected = [...keys].sort();
  if (actual.length !== expected.length || actual.some((key, index) => key !== expected[index])) {
    throw invalidRequest();
  }
}

function hex32(value, allowZero = false) {
  if (typeof value !== "string" || !HEX_32.test(value) || (!allowZero && /^0+$/u.test(value))) {
    throw invalidRequest();
  }
  return value;
}

function boundedHex(value, maximum, nullable = false) {
  if (nullable && value === null) return null;
  if (typeof value !== "string" || value.length === 0 || value.length > maximum ||
      value.length % 2 !== 0 || !/^[0-9a-f]+$/u.test(value)) throw invalidRequest();
  return value;
}

function littleEndianBigInt(value) {
  const hex = hex32(value, true);
  const bigEndian = Buffer.from(hex, "hex").reverse().toString("hex");
  return BigInt(`0x${bigEndian}`);
}

function parseOperation(value) {
  const operation = object(value);
  exact(operation, ["kind", "methodId", "publicKey"]);
  if (!METHOD_ID.test(operation.methodId)) throw invalidRequest();
  if (operation.kind === "add_authentication_method") {
    const key = object(operation.publicKey);
    exact(key, ["xHex"]);
    const x = Buffer.from(hex32(key.xHex), "hex").toString("base64url");
    return {
      kind: operation.kind,
      circuitId: "setVerificationMethod",
      args: [{ id: operation.methodId, typ: 1, publicKeyJwk: { kty: 3, crv: 0, x, y: "" } }, 1],
    };
  }
  if (operation.kind === "add_assertion_method") {
    const key = object(operation.publicKey);
    exact(key, ["xHex", "yHex"]);
    return {
      kind: operation.kind,
      circuitId: "setSchnorrJubjubVerificationMethod",
      args: [{
        id: operation.methodId,
        publicKey: { x: littleEndianBigInt(key.xHex), y: littleEndianBigInt(key.yHex) },
      }, 1],
    };
  }
  if (operation.publicKey !== null) throw invalidRequest();
  if (operation.kind === "add_authentication_relationship") {
    return { kind: operation.kind, circuitId: "setVerificationMethodRelation", args: [1, operation.methodId, 1] };
  }
  if (operation.kind === "add_assertion_relationship") {
    return { kind: operation.kind, circuitId: "setVerificationMethodRelation", args: [2, operation.methodId, 1] };
  }
  throw invalidRequest();
}

function parseRequest(value) {
  const request = object(value);
  exact(request, ["schemaVersion", "operation", "chain", "wallet", "controller"]);
  if (request.schemaVersion !== 1) throw invalidRequest();
  const chain = object(request.chain);
  exact(chain, ["contractStateHex", "contractAddressHex", "zswapChainStateHex", "ledgerParametersHex", "networkId", "timestampMillis"]);
  const wallet = object(request.wallet);
  exact(wallet, ["coinPublicKeyHex", "encryptionPublicKeyHex"]);
  const controller = object(request.controller);
  exact(controller, ["secretHex"]);
  if (typeof chain.networkId !== "string" || !NETWORK_ID.test(chain.networkId) ||
      !Number.isSafeInteger(chain.timestampMillis) || chain.timestampMillis <= 0) throw invalidRequest();
  return {
    operation: parseOperation(request.operation),
    contractState: boundedHex(chain.contractStateHex, MAX_CONTRACT_STATE_HEX),
    contractAddressHex: hex32(chain.contractAddressHex),
    zswapChainState: boundedHex(chain.zswapChainStateHex, MAX_ZSWAP_STATE_HEX, true),
    ledgerParameters: boundedHex(chain.ledgerParametersHex, MAX_LEDGER_PARAMETERS_HEX, true),
    networkId: chain.networkId,
    timestampMillis: BigInt(chain.timestampMillis),
    coinPublicKeyHex: hex32(wallet.coinPublicKeyHex),
    encryptionPublicKeyHex: hex32(wallet.encryptionPublicKeyHex),
    controllerSecret: hex32(controller.secretHex),
  };
}

function artifactRoot() {
  const root = process.env.OXID_MIDNIGHT_DID_CALL_ARTIFACTS_DIR;
  if (typeof root !== "string" || !path.isAbsolute(root) || path.normalize(root) !== root) {
    throw new ComposerError("unavailable", "DID composer is unavailable");
  }
  return root;
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
  contractModulePromise ??= import(pathToFileURL(path.join(root, "contract", "index.js")).href);
  return contractModulePromise;
}

function bytes(hex) {
  return new Uint8Array(Buffer.from(hex, "hex"));
}

export async function composeDidCall(value) {
  const request = parseRequest(value);
  const generated = await loadContractModule();
  const controllerSecret = bytes(request.controllerSecret);
  const witnesses = {
    localSecretKey({ privateState }) { return [privateState, controllerSecret]; },
    currentTimestamp({ privateState }) { return [privateState, request.timestampMillis]; },
    getSchnorrReduction({ privateState }) { return [privateState, [0n, 0n]]; },
  };
  const root = artifactRoot();
  const compiledContract = CompiledContract.make("did", generated.Contract).pipe(
    CompiledContract.withWitnesses(witnesses),
    CompiledContract.withCompiledFileAssets(root),
  );
  setNetworkId(request.networkId);
  try {
    const call = await createUnprovenCallTxFromInitialStates(
      new NodeZkConfigProvider(root),
      {
        compiledContract,
        circuitId: request.operation.circuitId,
        contractAddress: request.contractAddressHex,
        args: request.operation.args,
        coinPublicKey: request.coinPublicKeyHex,
        initialContractState: ContractState.deserialize(bytes(request.contractState)),
        initialZswapChainState: request.zswapChainState
          ? ZswapChainState.deserialize(bytes(request.zswapChainState))
          : new ZswapChainState(),
        ledgerParameters: request.ledgerParameters
          ? LedgerParameters.deserialize(bytes(request.ledgerParameters))
          : LedgerParameters.initialParameters(),
        initialPrivateState: {},
      },
      request.encryptionPublicKeyHex,
    );
    const serialized = call.private.unprovenTx.serialize();
    return {
      schemaVersion: 1,
      ok: true,
      operationKind: request.operation.kind,
      circuitId: request.operation.circuitId,
      unprovenTransactionHex: Buffer.from(serialized).toString("hex"),
      unprovenTransactionBytes: serialized.length,
    };
  } catch {
    throw new ComposerError("composition_failed", "DID call composition failed");
  } finally {
    controllerSecret.fill(0);
  }
}
