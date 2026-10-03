# DUST registration recovery artifact

The JSON wallet-profile provider persists the public DUST-registration recovery
record as `wallet-dust-registration-recovery.bin` beside its configured
`wallet-profiles.json` file. The location is therefore determined by the
profile-store configuration (including `OXID_PROFILE_STORE_PATH`); it is not a
user-exportable backup and must remain owner-private.

## Lifecycle and cleanup

The record contains only public workflow identity, revisions, and submitted
transaction identifiers. It never contains a seed, authorization challenge,
signed transaction, proof, or custody material. It is updated before a
submission may be broadcast and is removed when the application intentionally
abandons a recoverable record or cannot resume protected authorization after a
restart.

A normal cleanup is performed by the wallet runtime through its recovery-store
provider. Operators may remove this file only while the wallet is fully stopped
and after confirming that no DUST registration is submitting or awaiting
reconciliation. Removing it during an active submission intentionally makes a
subsequent submission fail closed rather than allowing an unrecorded broadcast.

Projection updates use a bounded latest-value persistence worker, so ordinary
wallet observation and UI publication do not wait for filesystem I/O. Superseded
non-submission records are coalesced. A `Submitting` checkpoint is instead pinned
until the executor reads it back; a later projection cannot replace it in that
window. Transaction submission is the durability boundary: the executor waits
within its operation deadline, reads the pinned record back, and broadcasts only
when it is the exact `Submitting` record for the authorized draft. The operation
also flushes its latest recovery projection before returning. A stalled worker,
failed write, timeout, or mismatched record therefore prevents the chain side
effect without freezing read-only wallet state.

## Ownership and failure policy

One wallet process owns a profile-store directory at a time. `save` uses the
owner-private atomic-store writer, and the recovery-store handle derived by a
repository serializes its in-process access. Separately constructed repository
instances do not share that mutex, and there is no cross-process lease. Running
two wallet processes or repository instances against the same profile-store
directory is unsupported: an operator must stop all but one before continuing
or before manual cleanup.

Malformed recovery bytes are reported as `Corrupt`; the runtime may discard
those bytes and rebuild state from the selected realm. Filesystem policy
failures (a symlink, wrong file type, or non-private permissions) are reported
as `Integrity`, are never deleted by recovery, and disable durable submission.
Transient I/O failures are `Unavailable` and likewise fail closed. Correct the
ownership or permissions problem while the wallet is stopped, then restart; do
not replace a suspicious path in a running process.
