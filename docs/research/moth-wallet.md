# moth-wallet — competitive study

- **Subject**: [`shieldedtech/moth-wallet`](https://github.com/shieldedtech/moth-wallet)
- **Studied**: 2026-08-20, against moth `c36e119a` (2026-08-19) and Oxid `319ca5d`
- **Why it matters**: `shieldedtech` is **Shielded Technologies**, the Input
  Output spinout that is Midnight's core technology partner. This is not a
  community wallet — it is the protocol partner's own reference
  implementation, which makes its interface choices *ecosystem-normative*
  whether or not Oxid adopts them.

> Self-declared status, quoted so it is not overstated here: *"Experimental
> and Unsupported… It is not supported. We do not maintain it, fix bugs, patch
> security issues or respond to support requests… It has not been audited."*
> Oxid should treat its **interfaces** as ecosystem signal and its
> **implementation** as unaudited reference code.

## The headline

The two projects made almost perfectly complementary bets:

- **moth-wallet** ships a complete Midnight **DApp connector** in a working
  Chrome extension, with in-browser WASM proving, published npm packages and
  signed release artifacts — and has **no identity layer at all**.
- **Oxid** has a deep, real SSI stack (did:midnight, OpenID4VCI/VP, SIOPv2,
  Compact ZK presentation proofs with independent verification), mobile
  platform-backed custody, and machine-enforced architecture — and has
  **zero DApp connectivity**: no code, and no decision record.

That second fact is the actionable one. Verified on `319ccd5`-era develop:

```
$ rg -ci "dapp|walletconnect|cip-?30|connector" crates apps
(no matches)
$ rg -li "dapp" docs/adr/*.md      # only incidental prose in 0015/0051/0067
```

Oxid has 100 ADRs and not one about how a DApp reaches the wallet. Midnight
DApps discover wallets by enumerating `window.midnight`; Oxid is invisible to
every one of them today, and nothing in the corpus says whether that is a
deliberate choice or an omission. See [issue #105](https://github.com/MediaNoxLabs/oxid/issues/105).

## Feature comparison

| Capability | moth-wallet | Oxid |
| --- | --- | --- |
| **Platforms** | Chrome MV3 extension (side panel), Node CLI, React/Ink TUI, browser lib. Firefox blocked (ledger WASM top-level await) | Android/iOS first (Dioxus), desktop secondary, headless NDJSON, MCP agent surface. Browser blocked (issue #101) |
| **Chains** | Midnight only; non-Midnight bridging an explicit non-goal | Midnight; Cardano deferred (ADR-0014 still Proposed) |
| **Key custody** | BIP-39/BIP-44 `m/44'/2400'`; scrypt N=2¹⁸ + ChaCha20-Poly1305 keystore; unlocked seed in `storage.session`. No hardware, no MPC | **iOS Keychain / Android Keystore sealing with user presence** (ADR-0071); opaque key references; key material cannot cross a port by type signature |
| **Shielded + unshielded** | Both, exercised against live networks | Both; shielded spend from adapter-private Zswap state (ADR-0079) |
| **DUST** | register/deregister/status, rate + capacity, dust-heal | Resumable checkpointed sync, cancellable, live-before-spend gate |
| **Contracts** | General: deploy, call, state query, token mint, maintenance verifier-key insertion with resumable batching | One product contract (Passport Vault) with canonical finalized replay |
| **Proving** | **User-selectable per network: in-browser WASM or proof server** — works today | Real Compact ZK presentation proving; mobile proving gated on device budgets (#30) |
| **DApp connectivity** | **Full `@midnight-ntwrk/dapp-connector-api` 4.0.1 — all 18 methods, `window.midnight.moth`, per-origin grants, revocation UI, functional `getProvingProvider`, plus a mock-DApp "Connector Lab"** | **None** |
| **SSI / identity** | **None.** Nearest is `deriveAppSecret(domain)` — HKDF-SHA-256 over a non-spending HD role key, origin bound by the wallet, approval-gated | did:midnight resolve + lifecycle, OpenID4VCI, SIOPv2 draft-13, OpenID4VP with real Compact proof **and independent verification**, 7-stage credential verification, Digital Passport, Jubjub opaque custody |
| **Backup / recovery** | 24-word phrase only | Portable + complete-wallet encrypted archive, hardened v3 Argon2id, native document pickers, one-transaction restore |
| **Networks** | mainnet/devnet/preview/preprod/qanet/local wired to **real endpoints and exercised** | Enum covers the same, but `"undeployed"` outnumbers `"mainnet"` 102:14 in crates; live routes are compile-time localhost/tailnet dev profiles |
| **Extensibility** | Isomorphic core + thin shells; core published to npm. Boundary not yet machine-enforced (their PR #23 open) | Machine-enforced hexagonal allowlist with default-deny, 14 crates with zero external dependencies, `unsafe` denied, `compile_error!` feature guards |
| **Agent surface** | JSON output on every command; daemon RPC with API keys + audit log; `MOTH_DAEMON_AUTO_APPROVE=1` auto-approves every confirmation | NDJSON v1 with capability discovery and per-method `secretsExposed`; MCP surface excluding every consent ceremony (ADR-0099) |
| **i18n** | **de/es/fr shipped**, CI-enforced key parity, no-hardcoded-strings guard | ADR-0085 label layer in place; no locales shipped |

## Maturity comparison

| Dimension | moth-wallet | Oxid |
| --- | --- | --- |
| Public age | 7 days (created 2026-08-13); private work back to ~2026-06 per spec/ADR dates | 9 days (created 2026-08-11) |
| Commits | 93 in 7 days | ~192 in 9 days |
| Contributors | **4–5 humans** + bots; one external PR merged; CODEOWNERS; DCO enforced | **1 human** + dependabot |
| Releases | **9 GitHub releases; 3 npm packages × 12 versions**; extension zips with SLSA in-toto + sigstore provenance; npm Trusted Publishing (OIDC, no token) | **0** |
| Distribution | Not on the Chrome Web Store — build and load unpacked | Nothing published |
| Adoption proxy | 6 stars, 2 forks | 0 stars, 0 forks |
| CI | 12 workflows (CI, OSV, CodeQL, OpenSSF Scorecard, changesets release), ~2 min | 9 workflows; ~21–28 min warm CI; nightly hermetic `nix flake check` |
| Tests | 93 vitest files incl. property/fuzz (`fast-check`) and CI guards — **coverage explicitly advisory, `continue-on-error`, cannot gate** | ~568 tests, ~500 hermetic; **80% line coverage enforced**; external known-answer vectors (BIP-32 Vector 4, official address codec) |
| Governance | Real MADR+validation ADR process, 5 ADRs — **their ADR index table is empty** | **100 ADRs** with maintained delivery-state table; published adversarial independent review (Discussion #37) |

## Where moth-wallet is genuinely ahead

1. **DApp connectivity — absent in Oxid, complete in moth.** All 18
   `ConnectedAPI` methods, an empty `NOT_IMPLEMENTED_METHODS` array, per-origin
   grants with a revocation screen, and a functional proving provider that
   resolves the DApp's key material in the page and executes in the wallet.
2. **It ships installable artifacts.** 192 Oxid commits and 0 releases,
   against 9 releases and 12 npm versions in the same calendar window. Signed,
   with SLSA provenance.
3. **In-browser WASM proving works today**, user-selectable per network.
4. **Live-network reality, with operational scars to prove it** — the whole
   request-metering/diagnostics feature exists because someone actually hit
   indexer 403 rate limits in anger.
5. **Cold-start sync solved as a product problem**: a published unfunded
   throwaway wallet's synced DUST state at a known height, bundled per network
   and applied *only* to wallets provably created after that height (a
   restored seed still walks full history, so it cannot hide its own funds).
   **Preprod first sync: ~78 minutes → ~29 seconds.**
6. **Bus factor and published supply-chain hygiene** — 4–5 humans, external
   contribution merged, OIDC publishing with no token in CI, attestations and
   a Scorecard badge a stranger can check.
7. **Field diagnostics**: a `debug.html` with per-host request totals, current
   and 1-minute-mean rates, **the busiest single second and when it occurred**,
   403s named explicitly, network failures counted apart from HTTP errors, and
   **"copy as curl" for the last five failures**. The most useful operational
   surface in either project.
8. **Shipped localization** (de/es/fr) with CI-enforced key parity.

## Where Oxid is ahead

1. **SSI is real and entirely absent from moth.** Zero credential surface
   there; here, DID lifecycle, OpenID4VCI/VP, SIOPv2, and Compact ZK
   presentation proofs that are independently verified before a token is
   emitted.
2. **Mobile-first with platform-backed custody** versus no mobile and an
   unlocked seed in session storage.
3. **Architecture enforced by machine, not convention** — default-deny
   dependency allowlist, 14 zero-dependency crates, workspace `unsafe` denial,
   feature guards, and key material that cannot cross a port because the
   signature forbids it. moth's equivalent boundary check is still an open PR.
4. **Fail-closed production composition, asserted by tests** — versus moth's
   unaudited/unsupported posture and its open mainnet-guard bug.
5. **Governance depth and honesty**: 100 ADRs with live delivery state, plus a
   published adversarial review of our own codebase.
6. **Encrypted portable and complete-wallet backup** versus phrase-only.
7. **Quality gates that actually gate**: enforced coverage, known-answer
   vectors, nightly hermetic checks. Their coverage cannot fail a build.
8. **A safer agent surface**: per-method `secretsExposed` and consent
   ceremonies excluded by construction, versus an auto-approve-everything
   environment variable.

## Roadmap implications, ranked

1. **Decide and record how a DApp reaches Oxid.** The mis-sequencing flag:
   100 ADRs, none on this, while the protocol's core partner has shipped the
   full connector. On mobile the analogue is a session/deep-link flow rather
   than an injected provider — that is a real architectural choice nobody has
   written down. The method surface is already specified for us by
   `dapp-connector-api` 4.0.1. → [issue #105](https://github.com/MediaNoxLabs/oxid/issues/105)
2. **Publish one installable artifact.** A signed `oxid-headless` binary or an
   internal mobile build converts an architecture into something a stranger can
   try. 0 stars versus 6 is what having nothing to install looks like.
3. **Attack cold-start sync.** A 78-minute first sync on mobile is not a slow
   start, it is an uninstall. ADR-0031/0032/0033 checkpoint machinery is most
   of the substrate; what is missing is the safety argument for a snapshot
   applied only to provably-newer wallets. Reason it out independently — do not
   adopt their snapshot.
4. **Get one capability onto a live public network end-to-end, with published
   evidence.** `"undeployed"` outnumbers `"mainnet"` 102:14; issue #97 shows a
   production placeholder that can encode a *public test-vector* key as a
   mainnet receive address. Unshielded NIGHT transfer against preprod is the
   cheapest candidate.
5. **Settle the wasm32 question deliberately.** Issue #101 says the Midnight
   adapter cannot compile for wasm32. Either fix it as a precondition for a
   browser surface, or record that DApp connectivity is mobile-session-only and
   web is out of scope. moth has already documented every hazard on that path.
6. **Engage with `deriveAppSecret` while its shape is still open.** Their
   ADR-0001 defines HKDF-SHA-256 over a non-spending role key with frozen,
   vendor-neutral `midnight:` v1 constants, explicitly so a future wallet-SDK
   implementation is byte-identical — and it is being proposed upstream. A
   Midnight-wide per-app identity primitive is being specified right now, by a
   wallet with no DID layer. Oxid is the natural place to bind such a secret to
   a **holder DID method** rather than a bare domain string. Silence here means
   the ecosystem primitive is settled without the SSI-native wallet in the room.
7. **Publish the quality story we already have.** Enforced coverage and nightly
   hermetic checks are strictly better than advisory, non-gating coverage — and
   we publish none of it. Codecov and Scorecard badges plus release
   attestations are about a day of work for credibility already earned.
8. **Add bounded local request diagnostics, and ship one non-English locale.**
   ADR-0080's closed codes are safer than their free-form metering but cannot
   answer "is the indexer refusing us, how often, at what burst rate". A
   local-only, opt-in, bounded counter is compatible with ADR-0013's
   telemetry-off stance.

**Two further sequencing flags.** ADRs 0084–0098 are fifteen consecutive
records on UI composition, brand packs, tokens, and demo drawers while the
load-bearing pillars (#30 mobile proving, #31 live vault adapter, #32 device
evidence) stay open — moth spent the same week on release plumbing and
third-party usability. And ADR-0014 (Cardano) and ADR-0016 (SSI components)
remain "Proposed — research gate before M1/M3" with 80+ later ADRs accepted on
top; worth confirming those gates are still real rather than quietly bypassed.

**A caution on velocity comparisons.** Both projects are heavily
agent-authored (moth has `AGENTS.md`, plan docs, and an `ai-assisted-label`
workflow; Oxid has a 152 KB `AGENT.md`). Commit counts say little about team
size. Contributor count still says something about bus factor.

## Sources

moth-wallet: repository metadata, contributor and release APIs, npm registry
entries for `@shieldedtech/moth-{wallet,cli,tui}`, and in-repo files —
`README.md`, `SECURITY.md`, `packages/core/src/wallet/keystore.ts`,
`packages/core/src/types/network.ts`,
`packages/extension/lib/connector/constants.ts`,
`packages/extension/entrypoints/injected.ts`,
`packages/extension/wxt.config.ts`, `packages/mock-dapp/README.md`,
`docs/adr/0001-deterministic-per-app-secret-derivation.md`,
`docs/spec/wallet-service/`, `docs/QUALITY_METRICS.md`,
`.github/workflows/ci.yml`, `AGENTS.md`, issue #25, PRs #22/#23/#32.
Shielded Technologies context: beincrypto and CCN coverage of the Midnight
partnership, shielded.io.

Oxid: this repository at `319ca5d` — `README.md`, `docs/site/src/status.md`,
`architecture.md`, `headless-protocol.md`, `testing-strategy.md`,
`docs/adr/README.md`, plus `rg` counts and `gh api` release/star data quoted
inline above.
