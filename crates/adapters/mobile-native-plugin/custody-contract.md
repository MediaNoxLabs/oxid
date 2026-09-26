# Secret-safe custody migration contract

## Scope and adoption

`oxid_adapter_mobile_native::custody` defines the shared, host-testable contract
for issue #779. It is **not yet the active platform transport**. Android/JNI
migration is #780 and iOS/Swift migration is #781. Existing `*_custody_json`,
`custodyJson`, `oxidCustodyJson`, and `storage-mobile` base64 parsing remain
legacy until those ordered slices replace them independently. Parent #760 must
remain open until both paths are removed and platform cleanup is verified.
There is no availability, authorization, or storage-format change in this slice.

This refines the adapter-private boundary described in ADR-0071; it does not
change application ports, the vault format, key protection labels, the 30-second
authorization session, or explicit user-presence requirements. Native FFI details
and any new unsafe code require review in their respective platform slices.

## Two channels, no compatibility decoder

- Custody is a separate mutable byte buffer, 1–524288 bytes. Never encode it as
  base64, JSON (including JSON arrays), Rust/JVM/Swift/Objective-C strings, or
  diagnostic text. Do not implement `CustodyTransport` by wrapping legacy calls.
- Rust receives directly into the fixed-size `CustodyBytes::receive` allocation,
  already zeroizing before the first secret byte is written. The callback must
  report that it filled the exact buffer or return a payload-free error; a short
  fill fails closed and wipes the allocation. The callback may not retain the
  borrowed pointer. `take_native` copies from an exclusive mutable slice and
  wipes that source on success and rejected size. Neither constructor accepts
  an already-allocated unprotected Rust `Vec` or `String`.
- Fixed-size boxed storage cannot reallocate secret-bearing capacity. It has no
  clone, serialization, display, or unprotected owned extraction API. Debug is
  always `CustodyBytes([REDACTED])`. Borrowed slices are only for synchronous
  protected decode/use. Rust's decoder must also retain parsed secret fields in
  zeroizing storage; this contract does not make arbitrary downstream copies safe.
- Control is at most 512 bytes of JSON. Exact fields: `version: 1`, `operation`
  (`inspect`, `initialize`, `unlock`, `load`, `save`, `lock`), `status`, optional
  `protection` (`operating_system` or `hardware_backed`). Unknown and duplicate
  fields, unknown enum values and versions fail closed. `payload`, `bytes`, and
  `handle` are forbidden even when null. No free-form errors or metadata exist.
- The caller supplies the expected operation; mismatches fail closed. Profile
  identity and exact native authorization are bound by the synchronous call,
  not trusted from a returned JSON field. Platform adapters must serialize access
  and bind delayed native callbacks to the exact call generation and profile.
- `decode_reply` consumes separate `CustodyBytes` and releases them only for a
  valid load/unlock success with a protection class. Any malformed control or
  failure drops the supplied material without interpolating parser/native errors.
- The typed `CustodyTransport` narrows inspection to `CustodyState` and keeps
  `Protection` coupled to load/unlock material and save/lock completions. A
  platform adapter cannot discard the reviewed native protection class while
  satisfying the shared interface.

An opaque native allocation may be used internally by a platform if necessary,
but no integer handle protocol is specified here. Its owner must provide
single-consumer transfer, checked length, generation/profile binding, idempotent
release and deallocation wiping before adapting it into `CustodyBytes`.

## Operation/event property matrix

| Command | Accepted completion | Recovery/error | Cancellation / timeout / drop |
| --- | --- | --- | --- |
| inspect | uninitialized without protection; locked/unlocked with protection; never bytes | payload-free error; never prompts | no material to retain |
| initialize | succeeded with protection, no returned bytes | already-initialized/denied/etc.; never overwrite existing custody | wipe native borrowed-input copies; Rust input owner wipes on drop |
| unlock | succeeded with protection and separate bytes | denied/locked/etc.; no bytes released; retry requires fresh native authorization | terminate exact generation, discard/wipe late bytes |
| load | succeeded with protection and separate bytes | locked fails; only explicit unlock recovers | never retain a late result for a subsequent call |
| save | succeeded with protection, no returned bytes | failure does not report persistence; existing atomic storage policy remains | wipe temporary plaintext; do not claim rollback of a completed native commit |
| lock | locked with protection, no bytes | payload-free error | invalidate native session and wipe owned temporary buffers |

Failure statuses are `unavailable`, `not_initialized`, `already_initialized`,
`authorization_denied`, `cancelled`, `timed_out`, `invalid`, `failed`; `locked`
without protection is a locked error. Failure responses never carry protection
or bytes. Unrecognized command/status combinations fail closed.

This shared validator is synchronous and stateless: no task registry, event
queue, replacement or supersession exists. It cannot accept a stale event on
behalf of a live call or detect a replay from JSON alone. Duplicate/stale/native
completion prevention is mandatory in each platform's call-generation owner,
not a claim provided by this validator. Rust material cannot be consumed twice
without making an explicit forbidden copy; it is non-cloneable and moved into
validation. Platform children must test cancellation, timeout, replacement,
stale callbacks, duplicate completion and subsequent fresh authorization.

## Ownership and cleanup

| Stage | Owner and required cleanup |
| --- | --- |
| native decrypt / input copy | native owns exclusively mutable storage, including temporary JNI arrays and Swift buffers; clear in finally/defer on every success/error and deallocation |
| native-to-Rust transfer | allocate zeroizing Rust destination before copying; validate bounds before allocation; release/wipe native source immediately after transfer, even if Rust rejects the result |
| Rust fill error or unwind | `Zeroizing<Box<[u8]>>` drops and wipes full initialized allocation; no partially filled object escapes |
| control rejection / authorization denial | consume/drop any Rust material; native still owns cleanup of every native allocation, including rejected oversize material |
| success decode/use | custody stays in zeroizing Rust allocation; downstream parsed keys remain zeroizing; drop immediately after protected operation |
| cancellation / timeout / abandoned caller | native generation owner discards/wipes late results rather than publishing to another call; ownership cannot be abandoned merely because the caller stopped waiting |
| final drop / deallocation | Rust uses zeroize's drop guarantee; native explicitly clears mutable storage before releasing it; process abort/OS copies are outside a best-effort in-process wiping guarantee |

Host tests exercise strict wire shape, operation/status checks, load/save/unlock
failure semantics, bounds, ingress errors, source wiping and redacted debug.
They do not inspect freed memory or prove JVM/Swift allocator behavior. Platform
qualification remains required, with no plaintext in test diagnostics/snapshots.
