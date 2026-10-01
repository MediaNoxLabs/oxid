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
only under `cfg(test)` inside the application tests.

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

The service captures approval generation and issue time before calling the
trusted port. It checks generation and time again after approval, before minting.
The exclusive expiry is checked again at consumption, against the injected
trusted clock. Times before the original issue time fail closed too. Lock,
profile changes, and lifecycle cancellation must call `invalidate()` on the
shared service; its generation change invalidates both issued and in-flight
approvals. Generation overflow permanently disables that composition rather
than wrapping. Poisoned state and clock failures return unavailable.

Mint/consume generation checks serialize with invalidation. The trusted prompt
runs outside this lock, allowing invalidation while approval is pending.
A successful consume is one atomic transition and returns only `()`. It must
occur immediately before the protected operation. Later operation failure never
restores authority: retry requires fresh approval. This primitive does not
atomically execute an external custody effect or provide durable recovery.

Mismatched attempts fail without consuming a matching capability; successful
consumption is irreversible. No registry, token persistence, or replay-history
cleanup is needed: single-use state lives inside the opaque capability.

## Command/event property matrix

| Command/event | Property and evidence |
| --- | --- |
| Approve then consume | Explicit trusted fixture succeeds once; default composition returns unavailable |
| Replay / duplicate | Second consume returns already-consumed |
| Concurrent consume | Barrier-synchronized eight consumers produce exactly one success |
| Profile or digest mutation | Independently constructed expected intent fails mismatch |
| Cross-operation substitution | Compile-fail doctest; internal defensive enum-mismatch test |
| Expiry / backward clock | Equality at expiry, late approval, and time before issue fail closed |
| Supersession / cancellation | Generation invalidation rejects issued and in-flight approvals |
| Replacement / recovery | A new service rejects old service authority; fresh approval is required |
| Stale approval result | Barrier-controlled generation or expiry change during prompt prevents minting |
| Generation exhaustion | No wrap; request and consume remain unavailable |
| Effect failure after consume | No restore API exists; replay test proves authority remains spent |
| Duplicate completion callback | Impossible: synchronous port returns once; asynchronous adapters must resolve to one result |
| Diagnostics | Exact redacted Debug assertions; closed payload-free error codes |

All state tests are deterministic and use no network, adapter custody, sleeps,
or real user approval. The full wallet application test suite also retains
existing backup/onboarding/recovery intent-pinning coverage.
