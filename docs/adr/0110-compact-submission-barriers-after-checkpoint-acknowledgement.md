# ADR-0110: Compact submission barriers after checkpoint acknowledgement

- Status: Accepted
- Date: 2026-09-13
- Amends: ADR-0035 and ADR-0079
- Source: issue #93

## Context

The bounded public submission journal retains included shielded-submission
barriers because an old owner-private shielded checkpoint could otherwise be
restored and permit planning a duplicate spend. Retaining every inclusion
forever eventually exhausts the bounded journal. Capacity must not weaken
persist-before-broadcast or private-state recovery guarantees.

## Decision

An included shielded-submission record is removable only after an authoritative
owner-private checkpoint acknowledgement has been durably saved and validated.
The acknowledgement binds the record to the same profile, network, derived-key
identity, source fingerprint, and a monotonic finalized checkpoint cursor. The
cursor must have advanced beyond the inclusion and the checkpoint must have
incorporated the spent inputs/nullifiers represented by that submission.

The authoritative checkpoint is written and reloaded successfully before its
matching public record is acknowledged or compacted. A public journal write
that records acknowledgement or removes an included entry occurs only after
that validation. Recovery evaluates both stores conservatively: a missing,
corrupt, rolled-back, torn, stale, or identity-mismatched checkpoint never
acknowledges a record. A crash at any point leaves either the old journal
barrier or an acknowledgement that is independently revalidated before it can
permit compaction.

`Broadcasting` and `OutcomeUnknown` records are never automatically evicted.
Same-draft persistence is monotonic: it cannot downgrade an included record to
an unresolved state or otherwise regress authoritative submission state.
Acknowledgement metadata is bounded and contains only identity fingerprints and
finalized cursor evidence; it contains no notes, nullifiers, transactions,
witnesses, endpoints, keys, or other secrets.

Existing journal records have no inferred acknowledgement. They remain barriers
until a matching current checkpoint creates a validated acknowledgement. Schema
migration is versioned and rejects malformed acknowledgement data rather than
silently treating it as compactable.

## Consequences

- A full journal of unresolved records remains unavailable before broadcast.
- A full journal of checkpoint-acknowledged included records can admit a new
  persist-before-broadcast record without discarding unresolved protection.
- Memory and JSON implementations must prove capacity, restart, rollback,
  corruption, stale identity, and acknowledgement/compaction crash boundaries.
