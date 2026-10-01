# Direct DID lifecycle approval

Issue #914 replaces caller-authored confirmation on direct DID update,
deactivation and signing with application-requested, sealed, single-use approval.
Publication retains its separate confirmation contract. Create, resolve,
inventory, persistence and forget are unchanged.

`DidService::from_ports` and `new` cannot approve protected operations. Production,
headless and ordinary `compose_in_memory()` have no approval authority, including
when development features are compiled. Direct headless methods return
`approval_unavailable`; legacy confirmation and alleged JSON capabilities are
unknown fields, not authority. The capability manifest reports protected DID
operations unavailable. Dioxus no longer presents a checkbox as authorization.

The only approving development implementation is
`identity/application/src/approval/development.rs`, compiled under
`development-approval` (or unit tests). Selection is an explicit source-level
call, not request input, prose or an environment variable. Composition exposes
`compose_in_memory_with_development_did_approval()` only under
`development-did-approval` (or unit tests); it uses `SystemClock` and `SystemSha256`.
Protocol positive tests explicitly select this same fixture. The repository
identity approval boundary contract guards producers, service injection and
fixture selection. There is no trusted production consent renderer yet.

## Exact intent and ordering

Profile and DID are domain-parsed. Update string fields and signing method IDs
are trimmed once; bare, `#fragment`, and same-DID full component IDs normalize
to the full `<did>#fragment` before approval (invalid components are rejected).
Payload bytes are never trimmed. The same normalized values
are used for approval reconstruction and effect. All three operation frames use
ordered fields with eight-byte big-endian byte lengths on **every** field:
`oxid.did.lifecycle`, `1`, operation, profile, DID, then variant and all its
fields (update), or method and payload (sign). Algorithms and relationships use
closed stable names. SHA-256 is provided by the platform hash port; no adapter
or cryptographic dependency points into the identity application. Deactivation
uses the same domain/version framing with operation `deactivate`, profile and DID.

The service reads the retained record, requests approval, reconstructs the intent,
then acquires a process-local per-(profile, DID) lock shared across service
instances. Under that lock it re-reads and compares the entire retained record
(including publication metadata), atomically consumes the matching capability,
and invokes the lifecycle port. The lock stays held through persistence (or
signing completion); unrelated DIDs remain concurrent. Approval callbacks run
outside the lock so a queued stale command fails the protected re-read. Failure never reaches the effect; persistence/effect failures never restore
spent authority. Replay, concurrent duplicate, expiry, stale generation, foreign
issuer and operation substitution remain guarded by the #913 capability service.
This is synchronous consume-before-effect, not an atomic transaction with an
external repository or custody system; no new cross-process guarantee is claimed.

Tests cover every update variant/field, signing scope/method/payload, framing,
component equivalence/invalid-input rejection, retained drift, a deterministic
stale-update/deactivation race through both effect and persistence, unrelated-DID
concurrency, unavailable defaults and capability rejection at
the effect boundary, plus explicit-fixture real-crypto lifecycle coverage.
Cancellation/replacement of an in-flight approval is represented by generation
invalidation; recovery cannot deserialize or revive process-local capabilities.

## Protocol authority

OID4VCI acceptance now mints one opaque, non-cloneable issuance authority only
after the retained session is revalidated as `awaiting_consent` for the exact
profile and issuance identifier. The authority moves by value through the
protocol issue request and holder-proof request. The identity-owned signing
boundary re-reads the current DID and exact authentication method under the DID
operation lock, checks the current algorithm, binds the authority to SHA-256 of
the exact canonical JWS signing input, consumes it, and only then reaches
protected signing.

Ordinary composition installs no issuance authority. Acceptance returns
`approval_unavailable`, leaves the session awaiting consent, and performs no
protocol, signing, or persistence effect. The existing explicit development DID
approval composition shares one process-local issuance authority between the
accepted-stage service and identity signing boundary so the positive standalone
fixture remains available. Request data and environment values cannot select
that authority.

SIOPv2 and presentation remain fail closed after removal of the obsolete direct
signing shortcut. Their accepted-stage authority integration, and a real trusted
producer tied to protection/profile transitions, remain subsequent work; this
OID4VCI slice does not change either protocol.
