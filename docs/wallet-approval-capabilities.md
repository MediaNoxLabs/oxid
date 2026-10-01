# Wallet approval capability foundation

`oxid-wallet-application` owns `WalletApprovalService`, the closed
`WalletApprovalIntent` operation set, and opaque operation-typed capabilities.
This is an additive, process-local authority primitive. Existing sign/delete,
backup, onboarding, and recovery commands are unchanged; no incoming adapter or
production composition is migrated by this foundation.

## Trust and composition

`WalletApprovalService::new(clock)` always uses an unavailable approval port.
It cannot mint approval. A trusted composition root may explicitly inject a
`TrustedWalletApprovalPort` with `with_trusted_port`. Implementations must obtain
explicit approval for the exact structured intent through a trusted surface;
request receipt, remote confirmation booleans, and caller-defined prose are not
approval. There is no production approving implementation, environment switch,
or development auto-approval path. The deterministic approving fixture exists
only under `cfg(test)` inside the application tests. The repository test
`tests/repository/wallet-approval-boundary-contract.test.mjs` scans every tracked
Rust source, rejects injection/port references outside the approval module and
its test fixture, and pins the sole production implementation to unavailable.
Negative fixtures cover incoming injection, alias imports, auto-approval and
loss of the test-only fixture gate. Adding a trusted adapter requires deliberately
updating this guard alongside its security review.

The port returns only `TrustedWalletApprovalError::{Denied, Unavailable}`;
the service maps these to its own error taxonomy. The intent enum remains
exhaustive so trusted prompt renderers must handle every operation explicitly;
public error enums are non-exhaustive so callers must fail closed on future
failures. The sealed operation markers still prevent downstream-defined operations.

The closed operations are direct signing and key deletion. Sealed Rust marker
types tie each request and capability to one operation. Cross-operation use is
rejected by the compiler; application consumption also compares the complete
closed intent defensively. Capabilities have no public constructor, serializer,
byte/token accessor, or `Clone`. Sharing references does not copy authority.
A unique service identity rejects capabilities from another composition, even
if profile, digest, clock, and generation coincide.

### Canonical digest precondition

`CanonicalApprovalDigest::from_sha256` wraps 32 bytes; it does **not** compute or
validate canonicalization, SHA-256, or user approval. The trusted caller must
hash an unambiguous, operation-domain-separated canonical command encoding
including the key reference and every payload/command field affecting the
operation. The command boundary must independently reconstruct that digest and
profile from its concrete command for consumption, rather than copy expected
fields from a capability or accept an incoming asserted digest.

Concrete canonical encoding, hash derivation, independent recomputation, trusted
preview rendering, and wiring lifecycle invalidation into sign/delete consumers
remain the separate consumer migration in issue #902. The foundation's tests
prove **digest binding and mutation rejection**, not concrete payload
canonicalization or protection of the existing commands.

## Lifecycle and consumption

The service captures approval generation and application-clock issue time before
calling the trusted port. Expiry is fixed at issue time plus 120,000 milliseconds,
including prompt time; callers cannot select an absolute expiry or extend TTL.
Checked timestamp overflow returns unavailable before prompting. It checks
generation and time again after approval, before minting.
The exclusive expiry is checked again at consumption, against the injected
trusted clock. Times before the original issue time fail closed with the distinct,
payload-free `approval_clock_went_backwards` diagnostic (no timestamps or IDs). Lock,
profile changes, and lifecycle cancellation must call `invalidate()` on the
shared service; its generation change invalidates both issued and in-flight
approvals. Generation overflow permanently disables that composition rather
than wrapping. Poisoned state and clock failures return unavailable.

Mint/consume generation checks serialize with invalidation. The trusted prompt
runs outside this lock, allowing invalidation while approval is pending.
The ordering decision is to retain `consume -> Result<(), WalletApprovalError>`
rather than issue a transferable consumed witness: a witness could itself be
retained beyond expiry or lifecycle invalidation without atomically fencing the
external effect. Consumers must execute `consume(...)?` immediately before the
protected effect, with no intervening prompt or asynchronous wait. The deterministic
`independent_capabilities_and_consume_before_failed_effect` test proves a failed
effect does not restore approval and an independently approved capability remains
usable. This is an explicit tested ordering contract, not a claim that Rust can
force an arbitrary consumer to call consume. Consumer migration must test the
actual custody call ordering and lifecycle boundary. Later operation failure never
restores authority: retry requires fresh approval. This primitive does not
atomically execute an external custody effect or provide durable recovery.

After issuer, generation and clock validation, consumption compares the
independently reconstructed expected intent before atomically spending the
capability. A mismatch conveys no authority and preserves the capability for its
approved operation; this avoids forcing another trusted prompt after a benign
caller mismatch. Equality is ordinary Rust equality, **not** claimed
constant-time, and the compared digest is not treated as secret. Each changed
digest byte and profile mismatch is tested. No incoming adapter may mint
authority. Successful consumption is irreversible. No registry, token
persistence, or replay-history cleanup is needed: single-use state lives inside
the opaque capability.

## Command/event property matrix

| Command/event | Property and evidence |
| --- | --- |
| Approve then consume | Explicit trusted fixture succeeds once; default composition returns unavailable |
| Replay / duplicate | Second consume returns already-consumed |
| Concurrent consume | Barrier-synchronized eight consumers produce exactly one success |
| Profile or digest mutation | Mismatch is rejected without spending matching authority; all 32 digest-byte mutations tested |
| Cross-operation substitution | Compile-fail doctest; internal defensive enum-mismatch test |
| Cloning / persistence | Clone compile-fail in application; Serialize/Deserialize compile-fail plus positive control in existing storage-json doctest host (no core serde dependency) |
| Expiry / backward clock | Equality at expiry, late approval, and time before issue fail closed |
| Supersession / cancellation | Generation invalidation rejects issued and in-flight approvals |
| Replacement / recovery | A new service rejects old service authority; fresh approval is required |
| Stale approval result | Barrier-controlled generation or expiry change during prompt prevents minting |
| Generation exhaustion / poisoned lock | No wrap or recovery; request, consume and invalidate remain unavailable |
| Effect failure after consume | No restore API exists; replay test proves authority remains spent |
| Duplicate completion callback | Impossible: synchronous port returns once; asynchronous adapters must resolve to one result |
| Diagnostics | Exact redacted Debug assertions; payload-free extensible error codes |

All state tests are deterministic and use no network, adapter custody, sleeps,
or real user approval. The full wallet application test suite also retains
existing backup/onboarding/recovery intent-pinning coverage.
