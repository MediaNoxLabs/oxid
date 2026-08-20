# ADR-0081: Clear Android JNI exceptions at the native boundary

- Status: Accepted
- Date: 2026-08-18
- Blueprint source: Sections 3–7, 12–13, 16–18, and 21
- Prototype source: `midnight-ledger` commit `074b1a4bccbfee1740ee188374b606a022ecef42`; the prototype does not supply an equivalent reviewed JNI recovery boundary
- Tracking: issues #2 and #41
- Amends: ADR-0070, ADR-0071, and ADR-0075
- Implementation state: every fallible Android JNI conversion in the shared native plugin clears a pending Java exception before returning a payload-free bridge failure; the Android emulator smoke injects a debug-only exception and then completes the existing standalone wallet flow

## Context

Oxid invokes the repository-owned Android activity from Rust worker threads for
QR, identity-link ingress, typed public-address export, native document
pickers, and device custody. The JNI crate reports a Java throw as an error but
does not clear the thread-local pending exception. Returning from the bridge
with that exception still pending makes later JNI operations on the same
thread fail and can eventually abort the process. A single Kotlin failure could
therefore poison unrelated wallet or custody work after Rust had already
converted it to `NativeBridgeError::Failed`.

The Java exception object and message can contain request, platform, or storage
details. Inspecting or describing it would create an unreviewed diagnostic
channel and conflict with ADR-0080's closed, payload-free runtime visibility.

## Decision

Route every fallible JNI operation in `oxid-adapter-mobile-native` through one
result mapper. On failure it checks for a pending Java exception, clears it
when present, discards the original error, and returns the existing closed
`NativeBridgeError::Failed` value. This covers activity method calls, Rust-to-
Java string allocation, JNI value conversion, and Java-to-Rust string reads.
Null return values remain a closed failure and require no exception handling.

The mapper must never call `ExceptionOccurred`, `ExceptionDescribe`, inspect a
throwable, retain a Java error, or place exception details in diagnostics. If
the JNI environment cannot check or clear an exception, the operation still
fails closed; no retry or fallback authority is inferred.

Add an explicit Android smoke-only Cargo feature. Its debug activity method
throws a message-free `IllegalStateException`; Rust must observe the normal
payload-free failure and immediately complete a second native activity call.
The existing emulator smoke then exercises the complete standalone wallet
journey, proving the process and subsequent native bridge calls remain usable.
Normal application builds have no Rust caller for the injection method, and a
non-debug build returns `unavailable` instead of throwing.

This is an outgoing-adapter recovery rule. It does not add a headless method,
application use case, domain type, user-visible diagnostic, or protocol
behavior.

## Consequences

- One Kotlin failure cannot leave the calling Rust thread in a poisoned JNI
  state after Oxid has returned a bridge error.
- Callers continue to receive only `Unavailable` or `Failed`; native exception
  types, messages, payloads, and stack traces do not cross the adapter.
- Android emulator evidence covers a real Java throw and subsequent full
  standalone use. It does not replace physical-device custody, camera, memory,
  or release evidence.
- New JNI calls in this plugin must use the common mapper. A direct fallible
  call is a security and process-liveness regression.

## Rejected alternatives

- Logging or describing the Java exception was rejected because it can expose
  secret-bearing native context and bypass the closed diagnostic taxonomy.
- Clearing only errors explicitly typed as `JavaException` was rejected
  because cleanup must also handle wrappers and later JNI conversion failures
  without coupling policy to dependency internals.
- Restarting the activity or process was rejected because it discards wallet
  work and avoids repairing the thread-local JNI invariant.
- Adding a headless recovery command was rejected because JNI state is an
  Android adapter concern, not wallet protocol state.
