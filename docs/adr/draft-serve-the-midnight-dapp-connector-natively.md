<!-- SPDX-License-Identifier: Apache-2.0 -->

# ADR-DRAFT: Serve the Midnight DApp connector natively

- Status: Proposed
- Date: 2026-08-20
- Blueprint: §§3–7, 11–13, 16, 18, 21
- Prototype source: `MediaNoxLabs/midnight-ledger` branch `dioxus-vc-demo`, HEAD `4c795b5`; `mobile-bench/dioxus-wallet/src/{bridge.rs,lib.rs,protocol.rs}` and `mobile-bench/wallet-core/src/{js_bridge.rs,vc_self_verify/mod.rs}`
- Reference contract: `@midnight-ntwrk/dapp-connector-api` v4.0.1 (mainnet); `enable()`, `state()`, and `balanceAndProveTransaction` were removed in v4.0.0
- Related: ADR-0027, ADR-0037, ADR-0050, ADR-0052, ADR-0059, ADR-0062, ADR-0067, ADR-0099; issues #105, #108, #109
- Analysis: `docs/research/midnight-dapp-connector.md`
- Implementation state: proposed; no adapter exists. The prototype implements four connector reads and defers balances; its Compact dependency is already retired in Oxid by ADR-0050.

## Context

A working DApp connection exists in the `dioxus-vc-demo` prototype and is used
by the credential-verification flow. It has three layers: an `mn-pkg://` custom
protocol serving a ~30 MB `include_dir!`-embedded JavaScript and WebAssembly
package tree into a WebView; a private `window.midnightWallet.*` JSON-RPC
channel over Dioxus `send`/`recv`; and a `postMessage` relay that forwards a
`window.midnight` host shim's calls from a dApp loaded **in an iframe** from
`MIDNIGHT_DAPP_URL`.

Four connector reads are implemented in Rust — `getConfiguration`,
`getConnectionStatus`, `getUnshieldedAddress`, `getShieldedAddresses` — and the
code says they *"mirror the `@midnight-ntwrk/dapp-connector-api` shapes."*
Balances and `getDustAddress` are deferred because *"they need sync
orchestration / a dust-address derivation not yet exposed by wallet-core."*
`getPublicKey`, `signData`, `didOp.prepareCall` and `didOp.submit` return
"not implemented yet".

Two prototype decisions are correct and should become invariants. Proving
payloads are kept **off** the RPC channel — a proof server runs on loopback and
the JavaScript reaches it over HTTP *"so we avoid bridging the proof preimage /
proving key payload through the JSON-RPC channel"* — which independently
reproduces the transport-side finding that 10–80 MB prover keys *"come close to,
or even exceed message size limits of different transport methods."* And *"the
seed never leaves Rust."*

Three prototype properties must not be carried across. The relay applies **no
method allowlist**, forwarding any caller-supplied method into
`window.midnightWallet.call`; one such method, `getControllerSecretKey`, returns
32 bytes of secret key as hex. The origin check **fails open** —
`if (!fromChildFrame && ev.source && ev.source !== window) fromChildFrame =
true;` — and the reply origin degrades to `"*"`. And `MIDNIGHT_DAPP_URL` accepts
a remote origin, so remote code reaches that surface.

Oxid has already decided against the carrier, twice, in accepted records that
cite this prototype directly. ADR-0067 rejects *"reusing the prototype iframe,
WebView JavaScript bridge, or `prepareVaultClaim`"* because it *"would move
credential and holder material outside the reviewed Rust custody boundary"*.
ADR-0052 rejects *"importing the prototype's JavaScript bridge, Node runtime,
iframe, or relative workspace lookup"* as a violation of *"the Rust-first and
reproducibility boundaries"*. ADR-0037 names the prototype's coupling to *"a
JavaScript bridge"*; ADR-0059 and ADR-0062 each record a way it failed.

The functional reason the bridge existed is now gone. The prototype needs
JavaScript for exactly one capability — Compact proof decode and verification
for `"midnight_compact_vc"` credentials, plus `call_did_circuit` — and it placed
that behind a `JsBridge` **port** with two adapters, noting that callers
*"don't care which transport they got"*. ADR-0050 supplies the native adapter:
Compact presentations are proved and verified in Rust, byte-for-byte conformant
to Compact runtime 0.15.0, from an authenticated Nix-produced artifact root,
with `prove_unchecked` forbidden. Oxid contains no JavaScript bridge at all.

## Decision

Oxid serves the Midnight DApp connector contract through a **native incoming
adapter**, and does not port the prototype's carrier.

**The contract.** Adopt the v4.0.1 method surface as the outward contract,
beginning with the reads the prototype proved — `getConfiguration`,
`getConnectionStatus`, `getUnshieldedAddress`, `getShieldedAddresses` — and
excluding the removed v3 methods. Balances and `getDustAddress` remain deferred
until sync orchestration and dust-address derivation exist, and the adapter
reports them as unavailable rather than returning a placeholder.

**The carrier.** The adapter is an incoming adapter over the existing NDJSON
protocol and `system.capabilities` manifest. It reuses `apps/oxid-mcp`'s
fail-closed filter unchanged in shape: a method is reachable only when its
manifest entry is `ready`, is not an alias, does not require confirmation, and
declares no `*Exposed` flag, with an independent authority-verb denylist as
defence in depth. No WebView, iframe, `postMessage` relay, custom URL scheme,
embedded JavaScript, or embedded WebAssembly package tree is introduced.

**Secrets.** No secret-bearing method is exposed. There is no
`getControllerSecretKey` analogue: the controller witness stays inside custody
per ADR-0037's opaque custody, and callers receive an operation, never material.

**Origin.** The adapter stamps the requesting origin itself and **fails closed**
on any origin it cannot establish. Origin is never accepted from the caller, and
every grant is per-origin, explicit, and revocable. This is the deliberate
inverse of the prototype's relay.

**Display.** For any connector-originated request that requires consent, the
wallet derives the confirmation text from the authoritative payload it holds and
refuses caller-supplied display strings (issue #108). A connector caller is
hostile-reachable by construction, so the current caller-supplied
`SensitiveOperationConfirmation` shape must not be reachable from it.

**Proving.** Prover keys, proof preimages, and other proving artifacts never
cross the connector channel. The proof server is reached directly, on loopback
unless ADR-0027's HTTPS requirement is satisfied, and its URI is advertised
through `getConfiguration` exactly as the prototype does.

**Reads.** Connector reads are served from wallet state and must not prompt;
only signing and proving may require consent, and then on every call. This
matches the normative split in the connector's CIP-30 sibling and the way
hardware wallets have served that contract for years — their device protocols
contain no query methods at all.

**Scope boundary.** Whether Oxid accepts a caller-built, byte-oriented
transaction for signing or balancing is **not decided here**. It is the subject
of issue #105 and requires its own record, because it would deliberately relax
`serializedTransactionExposed: false`. This ADR covers the read and
configuration surface plus the adapter's security properties; if #105 answers
"no", this adapter remains useful and the connector remains read-only.

## Consequences

The connector becomes reachable by any transport that already speaks the NDJSON
protocol — the headless surface, the MCP bridge of ADR-0099, and the QR approval
channel of issue #109 — instead of only from inside a WebView. One surface, one
filter, one audit.

The `oxid-mcp` filter becomes load-bearing for a **third** consumer, and the
first exposed to hostile web content. ADR-0099 already warns the manifest
*"becomes a load-bearing security boundary for a second consumer, which issue
#69 shows it is not yet fully ready for."* That warning now applies with more
force, and the belt-and-braces denylist is not optional.

Per-origin session state is genuinely new: the manifest is `composition_time`
static and cannot express grants, scopes, or revocation.

Deferring balances means a dApp expecting the full v4 surface will find part of
it unavailable. That is preferable to a placeholder, and it is the same choice
the prototype made for the same reason.

The prototype's dApp cannot be run unmodified, because its host shim targets the
`postMessage` relay this ADR declines to build. Bringing that dApp up against
the native adapter is separate work, and the shim's exact method list should be
read first — it lives outside the prototype repository.

## Rejected alternatives

- **Porting the iframe, `postMessage` relay, and `window.midnightWallet`
  channel.** Already rejected by ADR-0052 and ADR-0067 for moving credential
  and holder material outside the reviewed Rust custody boundary. Independently,
  the relay forwards unallowlisted methods and its origin check fails open, so
  adopting it would import a defect, not just a dependency.
- **Embedding the JavaScript and WebAssembly package tree.** ADR-0052 already
  declined to check in generated JavaScript and proving keys because it *"would
  duplicate large derivable artifacts and weaken source authentication"*; the
  prototype compiles ~30 MB into the binary via `include_dir!`. ADR-0050 makes
  it unnecessary.
- **Exposing a `getControllerSecretKey` equivalent.** Handing witness material
  to a caller inverts ADR-0037's custody boundary. The prototype's own comment
  claims the bytes *"never leave the embedded WebView"*, which is true of the
  process boundary and beside the point once the caller is remote.
- **Implementing the full v4 surface immediately.** Balances require sync
  orchestration that does not exist; asserting them would make the manifest
  untruthful, which is the failure mode issue #69 exists to prevent.
- **Deciding the byte-signing question here.** It changes a secret-hygiene
  invariant asserted across the whole manifest and deserves its own record
  rather than a clause in this one.
- **Treating the indexer and node as infrastructure to avoid.** They are
  network peer services whose endpoints Midnight publishes; depending on them is
  network participation. The proof server is the only trust-bearing service
  dependency, because it receives witness material, which is why it defaults to
  loopback.
