# The Midnight DApp connector prototype

- **Studied**: 2026-08-20
- **Subject**: `MediaNoxLabs/midnight-ledger`, branch `dioxus-vc-demo`, HEAD
  `4c795b5` (2026-06-22, *"Merge pull request #3 from
  yshyn-iohk/feat/vc-verification"*), directory
  `mobile-bench/dioxus-wallet/`
- **Question**: what does the working DApp connection actually consist of, and
  which parts of it should Oxid adopt?
- **Answer in one line**: **adopt the contract, not the carrier.** The method
  surface and two of its design decisions are directly reusable; the delivery
  mechanism is the one Oxid has already rejected in two accepted ADRs, and its
  relay fails open.

## What the prototype actually is

Three layers, not one.

**1. An embedded package server.** `src/protocol.rs` registers a Wry custom
protocol `mn-pkg://` that maps request paths into an `include_dir!`-embedded
copy of `assets/web/pkg/` — *"~30 MB on disk (mostly
`midnight-did-contract/dist`)"* — and serves each file with a correct
`Content-Type`. An import map injected into `<head>` rewrites
`@midnight-ntwrk/midnight-did-contract` and friends to `mn-pkg://` URLs, so the
WebView's own ES-module loader and `WebAssembly.instantiate` do the work:
*"No esbuild WASM plugin, no synthetic wrappers."* `include_dir!` rather than a
runtime filesystem read is deliberate — *"the app sandbox has no filesystem
access to the host's source tree"* on Android.

**2. A private Rust bridge.** `src/bridge.rs` (1,261 lines) runs a JSON-RPC
channel over Dioxus's `dioxus.send(...)` / `dioxus.recv()` and exposes it to JS
as `window.midnightWallet.<method>(...)` with promise semantics. The stated
principle is exactly right: methods are *"deliberately small — sign/derive
operations the wallet keeps in Rust because the seed never leaves Rust."*

**3. A DApp-facing relay.** `src/lib.rs` injects a script into the top document
that listens for `{ __type: "mn-host-req", id, method, args }` messages and
forwards them to `window.midnightWallet.call(method, args)`, replying with
`{ __type: "mn-host-res", id, result|error }`. The dApp itself is loaded **in an
iframe** from `MIDNIGHT_DAPP_URL` (`app.rs:2140`; default
plain HTTP on `localhost:3000`, and the prototype's own documentation shows a
remote HTTPS host as an alternative), and installs a `window.midnight`
host shim that posts those messages.

So the "Midnight DApp connection" is: **a remote dApp in an iframe, speaking the
standard connector API to a shim, relayed by postMessage into a Rust JSON-RPC
bridge, with a 30 MB JS/WASM stack available in the same WebView for Compact
circuit work.**

## The method surface, as implemented

Read from `bridge.rs`'s `run_method` dispatch. The comments themselves separate
the tiers, which is a good sign about the author's intent.

| Method | Tier | State |
| --- | --- | --- |
| `getConfiguration` | **Standard connector** | Implemented — returns `indexerUri`, `indexerWsUri`, `proverServerUri`, `substrateNodeUri`, `networkId` from `Network::config()` |
| `getConnectionStatus` | **Standard connector** | Implemented — always `"connected"` plus `networkId` |
| `getUnshieldedAddress` | **Standard connector** | Implemented via `Wallet::from_seed_hex` |
| `getShieldedAddresses` | **Standard connector** | Implemented — address, coin public key, encryption public key |
| balances, `getDustAddress` | Standard connector | **Deferred** — *"they need sync orchestration / a dust-address derivation not yet exposed by wallet-core"* |
| `getBech32Address` | Wallet-private | Implemented |
| `getProofServerUrl` | Wallet-private | Implemented |
| `ping`, `bundleError` | Diagnostics | Implemented |
| `vaultCreateLock`, `vaultDeposit`, `vaultClaim`, `vaultListLocks`, `vaultListCredentials`, `vaultTotalLocked` | Product-specific | Implemented |
| **`getControllerSecretKey`** | **Secret-bearing** | **Implemented** — returns `{ secretKeyHex }` |
| `getPublicKey`, `signData` | Wallet-private | `Err("… not implemented yet")` |
| `didOp.prepareCall`, `didOp.submit` | Contract pipeline | `Err("… not implemented yet (Compact runtime bridge)")` |

The comment above the standard block is worth quoting because it names the
target contract precisely: *"These mirror the
`@midnight-ntwrk/dapp-connector-api` shapes."*

## Two design decisions worth adopting outright

**1. Keep proving payloads off the RPC channel.** From the module doc: the
prototype spawns `midnight-proof-server` on `127.0.0.1:0` at startup and the JS
bundle *"talks to it via the same HTTP protocol upstream packages already use,
so we avoid bridging the proof preimage / proving key payload through the
JSON-RPC channel."*

This is independent corroboration of the conclusion reached from the transport
side in [web-bridge-architecture.md](web-bridge-architecture.md): the connector
spec warns that 10–80 MB prover keys *"come close to, or even exceed message
size limits of different transport methods."* Two people reasoning from
different directions arrived at the same invariant, which is the strongest kind
of evidence available for a design rule. **It should be written down as an
invariant rather than rediscovered a third time.**

**2. Keep the seed in Rust and expose operations, not material.** *"the seed
never leaves Rust"* is the correct boundary and matches ADR-0037's opaque
custody. The prototype states the principle and then breaks it in exactly one
place — see below — which is itself instructive.

## What is wrong with the carrier

These are prototype-grade problems in prototype code, and naming them is not a
criticism of the prototype: it exists to prove a flow works, and it does. They
matter only because the question on the table is what to *adopt*.

**The relay has no method allowlist.** It forwards whatever `d.method` the
iframe sends:

```js
const result = await window.midnightWallet.call(d.method, d.args || {});
```

So the embedded dApp can call any bridge method, including
`getControllerSecretKey`, and receive 32 bytes of secret key as hex. The
method's own comment says *"The 32 bytes never leave the embedded WebView"* —
true of the OS process boundary, and beside the point once the iframe is remote,
because the JS context **is** the boundary that matters. Compare Oxid's
`apps/oxid-mcp`, which filters a manifest by `status`, alias, `confirmationRequired`,
six `*Exposed` flags, **and** an independent authority-verb denylist. The relay
has none of those.

**The origin check fails open.** The hardening enumerates child frames, then:

```js
if (!fromChildFrame && ev.source && ev.source !== window) fromChildFrame = true;
```

The comment is admirably honest about why — *"so this hardening can't break
legitimate dApp -> wallet messaging"* — but the effect is that when frame
enumeration is unavailable, anything that is not a self-post is **accepted**.
The reply origin degrades the same way: `const replyOrigin = (ev.origin &&
ev.origin !== "null") ? ev.origin : "*"`. A fail-open origin check on a channel
that reaches key material is the single thing that must not be carried across.

**The dApp origin is configurable and remote.** `MIDNIGHT_DAPP_URL` defaults to
plain HTTP on `localhost:3000` and is documented with a remote HTTPS example.
Whoever controls that origin controls code with access to the whole relay
surface.

**The 30 MB embedded tree is a reproducibility and size problem**, not just an
aesthetic one: it is compiled into the binary by `include_dir!`, and it is
generated JavaScript plus WebAssembly that ADR-0052 already declined to check
in — *"Checking generated JavaScript or proving keys into Git would duplicate
large derivable artifacts and weaken source authentication."*

## Oxid has already decided this, twice

This is the decisive context, and it is the reason this study recommends
adopting a contract rather than porting code. The prototype's bridge is not an
unexamined option — **it is a named, rejected alternative in accepted ADRs**,
and ADR-0067 cites this very file as its prototype source (`src/bridge.rs`, at
commit `074b1a4`).

- **ADR-0067**, Rejected alternatives: *"Reusing the prototype iframe, WebView
  JavaScript bridge, or `prepareVaultClaim` would move credential and holder
  material outside the reviewed Rust custody boundary."*
- **ADR-0052**, Rejected alternatives: *"Importing the prototype's JavaScript
  bridge, Node runtime, iframe, or relative workspace lookup would violate the
  Rust-first and reproducibility boundaries."*
- **AGENT.md:130**: ADR-0067 *"connects the typed contract-state and retained
  vault-call lifecycle to Dioxus without WebView/iframe/JavaScript bridges."*
- **ADR-0037**: the prototype's *"mutable behavior is coupled to Dioxus,
  proving code, and a JavaScript bridge."*
- **ADR-0059** and **ADR-0062** each record a specific way the JS bridge failed
  — the latter *"failed when the number of supplied"* inputs varied.

Oxid's whole architecture is the result of mining this prototype and
reimplementing it in Rust on purpose. A proposal to bring the bridge back needs
to argue against five accepted decisions, and this study does not think that
argument can be won.

## What to do instead

The contract is the valuable part, and it can be served natively.

1. **Adopt the v4 connector method surface as an outward contract**, noting it
   moved under us: `enable()`, `state()` and `balanceAndProveTransaction` were
   **removed in v4.0.0**, and mainnet runs **4.0.1**. The prototype's four
   implemented reads map cleanly onto it.
2. **Serve it as an incoming adapter over the existing NDJSON protocol and
   capability manifest**, reusing `oxid-mcp`'s manifest-derived fail-closed
   filter, rather than as a JS channel.
3. **Serve reads from wallet state, not from a round trip.** The prototype
   already deferred balances for exactly the reason that matters — they need
   sync orchestration — and CIP-30 makes the split normative: reads must not
   prompt, signing must prompt every time. Ledger and Trezor have run this way
   for years; device protocols contain no query methods at all.
4. **Carry no secret-bearing method.** There is no `getControllerSecretKey`
   analogue in a design where the witness stays inside custody and only an
   operation is exposed.
5. **Bind the origin in the wallet, fail closed**, and derive confirmation text
   from the authoritative payload rather than accepting it from the caller
   (issue #108).
6. **Write the proving-payload rule down as an invariant**: prover artifacts
   never cross the connector channel; the proof server is reached directly, and
   on loopback unless ADR-0027's HTTPS rule is satisfied.

The proposal is drafted as
[`docs/adr/draft-serve-the-midnight-dapp-connector-natively.md`](../adr/draft-serve-the-midnight-dapp-connector-natively.md).

## Open questions this study could not settle

1. **The `window.midnight` shim itself is not in this repository.** It lives at
   `passport-vault-dapp/lib/midnight/mobile-bench-host.ts` per the relay's
   comment, which was not part of the clone. Its exact method list and error
   mapping should be read before finalising the adapter's surface.
2. **`didOp.prepareCall` / `didOp.submit` are unimplemented**, and their
   comments describe the intended split — JS runs the Compact circuit against
   on-chain state and returns a serialised `ContractCallPrototype`; Rust wraps
   it in an `Intent`, balances dust, proves and submits. **That split is the
   crux of the byte-signing question in issue #105**, and the prototype stops
   exactly where the hard decision starts. Whether Oxid can produce that
   prototype in Rust — rather than accepting one from JS — determines whether a
   connector is possible at all under current invariants.
## The Verifier flow, and why the JS dependency is already retired

This was going to be an open question. It resolves, and it is the most
important finding in the study.

**The prototype's JS dependency is narrow and enumerable.** `wallet-core` owns
OID4VP and OID4VCI natively (`src/oid4vp_client`, `src/oid4vci_client`,
`src/vc_self_verify`), and `vc_self_verify` branches on credential format:

- `"midnight_compact_vc"` (digital passport) — *"calls
  `decodeDigitalPassportProof` and `verifyDigitalPassportIssuanceProof` through
  the wallet's JS bridge."*
- **All other formats** — *"the original path that re-resolves the issuer DID
  on-chain, strips the embedded `proof` map from the CBOR body, and checks the
  Ed25519 signature via `SecretStorage::verify`"* — pure Rust.

So JS is needed for exactly one thing: **Compact proof decode and verification**
(plus `call_did_circuit` for contract calls).

**And the prototype already put that behind a port.** `wallet-core/src/js_bridge.rs`
defines a transport-agnostic `JsBridge` trait with two adapters — a
`DioxusEvalBridge` over the production WebView, and a `NodeChildBridge` that
spawns a Node harness and *"pipes newline-delimited JSON-RPC over
stdin/stdout"*. The module doc states the payoff explicitly: *"Both implement
the same `JsBridge` trait, so the Compact-runtime-driven flows
(`call_did_circuit`, etc.) consume the trait and don't care which transport they
got."*

That is hexagonal architecture, in the prototype, at exactly the right seam.
The prototype's own design tells you how to remove the JavaScript: implement the
same port with a native adapter.

**Oxid already did.** Two checks confirm it:

```
$ grep -rl "JsBridge\|js_bridge\|eval_bridge\|midnightWallet" crates apps
(no matches)
```

and **ADR-0050 "Prove and independently verify Compact presentations"**
(Accepted, 2026-08-14) does the work natively: the presentation `ProofPreimage`
is constructed *"in Rust only after credential, opening, selection, time,
current-control, and holder-proof checks succeed"*, a conformance vector must
*"tagged-serialize byte-for-byte identically to generated Compact runtime
0.15.0 for the same inputs"*, artifacts come from *"one Nix-produced,
self-contained artifact root"* with authenticated byte sizes and SHA-256
digests, *"no network fetch"*, and `prove_unchecked` is forbidden.

**Therefore the single capability the prototype's JavaScript existed to provide
is already available in Oxid, in Rust, with stronger guarantees than the
prototype had.** There is no remaining functional argument for the bridge — only
the cost of writing the connector adapter, which is the work this proposal
scopes.

## Open questions this study could not settle

1. **The `window.midnight` shim itself is not in this repository.** It lives at
   `passport-vault-dapp/lib/midnight/mobile-bench-host.ts` per the relay's
   comment, which was not part of the clone. Its exact method list and error
   mapping should be read before finalising the adapter's surface.
## The native contract-call path exists

Questions 2 and 3 of this study are answered, and the answer is better than the
prototype's own plan.

`MediaNoxLabs/midnight-identity`, branch `rust-codegen` (the repository default,
HEAD `5cb0590`), is *"Midnight DID implementation in Rust"* — seven crates,
~18.5k lines, and a `grep` for `wasm-bindgen|js-sys|node|napi` across every
crate manifest returns **nothing**. It is built on Compact-to-**Rust** codegen:
`crates/midnight-did-runtime` declares `compact-runtime`, described as *"the
compact-runtime symbol the generated contract code calls into"*, materialised
*"from the codegen-rust compact flake input"*, with `midnight-onchain-runtime`
and `midnight-base-crypto` kept as *"the load-bearing pair the generated.rs
path-mounted output names directly"*.

The layout is hexagonal — `midnight-did-domain`, `-api`, `-runtime`, `-method`,
`-uniffi`, `-cli` — with `Backend`, `PrivateStateStore`, and a generated
`Witnesses<PS>` as ports.

**The important difference is the type.** `DidContractCall` is a typed enum:

```rust
pub enum DidContractCall {
    ReadLedger,
    RotateControllerKey    { new_public_key: [u8; 32] },
    RecoverControllerKey   { new_public_key: [u8; 32] },
    SetVerificationMethod  { /* ledger-shaped method */ },
    // …
}
```

The prototype's plan for `didOp.prepareCall` was for **JavaScript to run the
circuit and hand Rust an opaque hex-serialised `ContractCallPrototype`** to
wrap, balance, prove, and submit sight-unseen. The native path gives Rust a call
it constructs and understands itself, with the authorization distinction visible
in the type system — `RotateControllerKey` is controller-authorized while
`RecoverControllerKey` is recovery-authority-authorized, and the on-chain
circuit checks a different signature for each.

**So the wallet never needs to accept a caller-built opaque blob for DID
operations.** That materially narrows what issue #105 still has to decide: the
secret-hygiene tension was about accepting foreign bytes, and for this path
there are none.

**One consequence to size before adopting**, flagged rather than resolved: seven
new crates carrying `compact-runtime`, `midnight-onchain-runtime`, and
`midnight-base-crypto`, plus a path-mounted `generated.rs` from a Compact flake
input. Oxid's 16 core crates must have zero external dependencies and
`scripts/check-architecture.sh` enforces per-crate allowlists, so these belong
in an adapter tier with an explicit entry — and the Nix closure cost wants
measuring against the CI budget first, not after.

## Why a WebView is still required

Retiring the JavaScript *cryptography* does not retire the JavaScript *runtime*,
and conflating those was the error in this study's first draft.

Supporting arbitrary third-party DApps means hosting third-party **web** code.
A DApp is a web application; native Rust does not change that. The architecture
that follows is:

```
oxid-wallet → DAppAPIHandler → JsBridge → WebView → Midnight DApp
                    ↑                               (injected connector object)
              security boundary
```

The accepted records that reject a JavaScript bridge are about the wallet's own
flows — ADR-0067 because the prototype bridge *"would move credential and holder
material outside the reviewed Rust custody boundary"*, ADR-0052 because
*importing the prototype's* bridge and Node runtime breaks Rust-first and
reproducibility. Neither addresses hosting a counterparty.

The line that matters is between a WebView used **as a component of the wallet's
own cryptography** — a custody hole, and now unnecessary — and a WebView used
**as a sandbox for a counterparty receiving only what the wallet chose to
emit**. The first stays rejected. The second is what "support different DApps"
means, and no credential, holder, witness, or key material crosses it.

Which makes every relay finding above *more* load-bearing, not less: the
`DAppAPIHandler` is the single security boundary, so the method allowlist,
per-origin grants, fail-closed origin binding, and wallet-derived confirmation
text all belong there — never in the injected script, which is attacker-reachable
by definition.
