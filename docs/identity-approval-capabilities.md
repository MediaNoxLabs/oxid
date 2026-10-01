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
are trimmed once; payload bytes are never trimmed. The same normalized values
are used for approval reconstruction and effect. All three operation frames use
ordered fields with eight-byte big-endian byte lengths on **every** field:
`oxid.did.lifecycle`, `1`, operation, profile, DID, then variant and all its
fields (update), or method and payload (sign). Algorithms and relationships use
closed stable names. SHA-256 is provided by the platform hash port; no adapter
or cryptographic dependency points into the identity application. Deactivation
uses the same domain/version framing with operation `deactivate`, profile and DID.

The service reads the retained record, requests approval, reconstructs the intent,
re-reads and compares the entire retained record (including publication metadata),
and atomically consumes the matching capability immediately before the lifecycle
port. Failure never reaches the effect; persistence/effect failures never restore
spent authority. Replay, concurrent duplicate, expiry, stale generation, foreign
issuer and operation substitution remain guarded by the #913 capability service.
This is synchronous consume-before-effect, not an atomic transaction with an
external repository or custody system; no new cross-process guarantee is claimed.

Tests cover every update variant/field, signing scope/method/payload, framing,
normalization, retained drift, unavailable defaults and capability rejection at
the effect boundary, plus explicit-fixture real-crypto lifecycle coverage.
Cancellation/replacement of an in-flight approval is represented by generation
invalidation; recovery cannot deserialize or revive process-local capabilities.

## Deferred protocol authority

OID4VCI, SIOPv2 and presentation adapters only lose the obsolete direct-command
confirmation construction. Public challenges, payloads and accepted-stage prose
are not approval authority. Normal compositions therefore cannot use their
former direct-signing shortcut. Moving authority to accepted protocol stages,
and integrating a real trusted producer with protection/profile transitions,
remain #915 and subsequent trusted-composition work. No challenge-signing port
semantics are changed here.
