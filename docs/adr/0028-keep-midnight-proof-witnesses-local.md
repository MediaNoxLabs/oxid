# ADR-0028: Keep Midnight proof witnesses local by default

- Status: Accepted
- Date: 2026-08-12
- Source: Blueprint §§3–5, 7–8, 12–13 and [issue #12](https://github.com/MediaNoxLabs/oxid/issues/12)
- Prototype source: `midnight-ledger` commit `074b1a4bccbfee1740ee188374b606a022ecef42`
- Implementation state: Native local DUST proving, bounded authenticated cache, cancellation boundaries, interoperability harness, and iOS/Android measurements implemented; production custody remains fail-closed
- Supersedes: ADR-0027 only where it deferred local proving or described remote proving as the sole completion path

## Context

ADR-0027 completed the development transaction path with a compatible remote
proof server. That boundary is useful for explicit standalone development, but
the request body contains private proof witnesses. A production-capable wallet
must not disclose those witnesses merely to avoid local CPU or memory cost.

The reviewed prototype also had an in-process prover. It depended on repository-
relative paths, ambient cache selection, unbounded on-demand download, mutable
proof-source experiments, and global process configuration. Those properties
cannot be copied into an independently versioned public mobile wallet.

The immutable ledger baseline already resolves the published
`midnight-proofs 0.7.3`, `midnight-circuits 6.3.0`, and
`midnight-zk-stdlib 1.3.0` packages through the ledger's proving feature. The
missing direct API is `midnight-zkir 2.1.0`, which is not published and is
maintained in the same official `midnight-ledger` repository and revision as
the transaction types.

## Decision

Add `midnight-zkir 2.1.0` as a native-only dependency from the official HTTPS
Git source at the full ledger revision
`d9414884db9da9e9b1f6f3a7f742d79a5732f817`, with default features disabled.
Use its `LocalProvingProvider` behind the existing Midnight transaction adapter;
do not expose ZKIR or ledger proof types through application ports.

Standalone composition has two explicit, mutually exclusive proving modes:

- `OXID_MIDNIGHT_PROVING_CACHE_DIR` selects private local proving; or
- `OXID_MIDNIGHT_PROOF_SERVER_URL` selects the development-only remote prover.

Supplying neither or both while submission routes are present fails startup.
Normal production composition remains fail-closed until native custody and
production chain configuration are available. Any future production-capable
composition must select local proving by default; remote proving always remains
an explicit development mode.

The local cache is an absolute app-private path supplied by platform
composition. Relative paths and `.`/`..` components are rejected. The adapter
creates an owner-only root, rejects symlinks and non-regular entries, allows at
most 32 entries and 8 MiB total, and downloads through owner-only temporary
files followed by an atomic rename. It accepts only the fixed official HTTPS
source and the exact allow-listed SHA-256 digests from the pinned ledger plus
the pinned base-crypto parameter manifest. Ambient HTTP proxy variables are
ignored so they cannot silently redirect proof material. Individual artifact
and request time limits are enforced.

The DUST IR is parsed before proving and must declare exactly k=13. Its observed
model contains 5,646 rows. Any k change is a proof-complexity review event, not
an automatic dependency update. The selected cache contains 3,752,829 bytes:
the DUST prover key, verifier key, IR, and `bls_midnight_2p13` public parameters.
No proving artifacts are committed to this repository.

Local completion runs on the existing named submission worker, never the
incoming or Dioxus UI thread. One gate per live adapter permits only one
local proof at a time, bounding simultaneous high-memory work. Dropping the
caller signals cancellation. The worker checks that signal before network
steps, before proving, after proving, and immediately before node submission;
a known pre-broadcast cancellation restores the authorized draft. The upstream
Halo2 proof future is a monolithic CPU operation, so this decision does not
claim hard mid-proof preemption. Once broadcast may have occurred, cancellation
cannot make the draft retryable and the unknown-outcome rule from ADR-0027
still applies.

An opt-in `proving-bench` example constructs one deterministic synthetic DUST
spend, proves and seals it with operating-system entropy, tagged-serializes it,
and requires byte-identical decode/re-encode. It never contacts a node. The
2026-08-12 release measurements with preloaded authenticated artifacts were:

| Target | First proof | Warm proof | Peak RSS | Proof | Sealed transaction | Harness binary |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| macOS arm64 host | 361 ms | 287 ms | 113,152 KiB | 2,912 B | 3,274 B | 13,645,904 B |
| iPhone 17 Pro simulator, arm64 | 458 ms | 365 ms | 118,320 KiB sampled | 2,912 B | 3,273–3,274 B | 13,528,128 B |
| Android arm64 emulator | 1,750 ms | 1,543 ms | 86,624 KiB | 2,912 B | 3,273–3,274 B | 13,898,304 B |

The first/warm labels describe the first and second proof in one process with a
preloaded cache. Network download time is deliberately separate and was not
measured in the proxy-only host environment because weakening the no-proxy
trust boundary would invalidate the test.

## Consequences

- Private DUST witnesses can remain on the device in the complete standalone
  transaction flow.
- Remote proving remains available for explicit development but can no longer
  become an accidental production default.
- Cache corruption, substitution, unexpected growth, and symlink redirection
  fail closed with sanitized application errors.
- Proving adds material native binary and memory cost; production devices still
  require representative release profiling when the native custody composition
  is enabled.
- Cooperative cancellation avoids broadcast after a cancelled request at safe
  boundaries, but cannot reclaim CPU during the current monolithic Halo2 call.
- Browser proving remains outside this decision and is tracked separately.
