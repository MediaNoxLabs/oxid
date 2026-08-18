# Security policy

## Reporting a vulnerability

Please report suspected vulnerabilities through GitHub private vulnerability
reporting for this repository. Do not open a public issue and do not include
wallet secrets, credential claims, identifiers, or exploit details in public
channels.

Include the affected revision, platform, reproduction steps, impact, and a
minimal proof of concept where safe. Maintainers will acknowledge and triage the
report as promptly as possible and coordinate disclosure after a fix is ready.

## Current security posture

Oxid is **not production-ready**: do not point it at real assets, production
identity keys, or externally issued credentials. This section states what the
delivered code actually protects and persists. It must be updated in the same
change as any slice that alters custody, persistence, backup, or composition
behavior; the per-decision delivery state in
[`docs/adr/README.md`](docs/adr/README.md) is the authoritative source it
summarizes.

### Custody

Normal iOS and Android composition seals a multi-curve software vault
(Ed25519, P-256, Jubjub, secp256k1) behind the platform keystore — Apple
Keychain and Android Keystore — with device user-presence authorization
required for unlock (ADR-0071). Blocking authorization work runs on a
dedicated background thread, never the UI executor (ADR-0077). Non-mobile
production composition still reports protected custody unavailable. Key use
everywhere is expressed through opaque references and key-operation ports; no
seed, mnemonic, private key, or scalar crosses a DTO, log, fixture, or the
headless protocol, and tests reject secret-bearing requests without echoing
them.

The `storage-dev` signing adapter and in-memory stores are **not** secure
storage: they are selected only by explicit standalone/headless composition,
report `development_only`, and intentionally forget their roots on restart.
Contradictory custody feature combinations fail to compile.

### What is persisted

Delivered slices persist the following, each in a bounded, owner-only,
symlink-rejecting, atomically written store:

- public wallet-profile metadata and public account associations (network id
  and HD indices only);
- public DID documents, in a separate store;
- credential records encrypted with XChaCha20-Poly1305 (schema v3), with
  size-bounded, debug-redacted private material and detached proofs;
- the standalone Passport Vault conformance ledger (owner-private,
  explicitly process-local semantics; never a chain claim);
- the public transaction submission journal (fee, hash, anchor, state — no
  signed transaction, proof, witness, key, or authorization payload);
- public account checkpoints and adapter-private DUST/shielded sync
  checkpoints (cursors and public progress; never seeds or secret scalars).

### Backups

Encrypted wallet backups may leave the device through operating-system
document pickers (ADR-0075/0076). The envelope authenticates its version,
algorithm identifiers, KDF parameters, salt, nonce, and lengths as AEAD
associated data; each readable format version maps to exactly one accepted
Argon2id policy, so a header cannot request arbitrary work (ADR-0078).
Current complete-wallet exports use 65,536 KiB / t=3; legacy packages remain
read-only recoverable at their recorded parameters. Recovery requires an
empty destination and compares restored custody in constant time.

### Proofs and live data

Zero-knowledge presentation proving executes only in explicit native headless
composition against digest-authenticated artifacts; everywhere else consent
fails closed at `proof_unavailable`, and no signature, local computation, or
fixture is ever substituted for a proof. Indexer-supplied reads are labeled
`indexer_supplied_not_proven` and never authorize contract calls;
authenticated contract state comes from deterministic replay of finalized
node history. Transaction submission persists its public attempt before
broadcast, never blind-retries an ambiguous outcome, and reconciles against
finalized history only.

### Enforced constraints

- no raw private key or seed material in UI/application DTOs;
- platform time and randomness behind explicit ports;
- persistence behind owned repository ports;
- Dioxus isolated as an incoming adapter;
- telemetry disabled by default;
- no secrets or claims in logs;
- `unsafe` denied workspace-wide with a single reviewed JNI exception;
- a default-deny architecture gate over every workspace crate;
- dependency and advisory checks independent from tests, with SHA-pinned CI.

## Supported versions

Until the first release, only the latest commit on `develop` receives security
fixes. Release support policy will be published before a stable version.
