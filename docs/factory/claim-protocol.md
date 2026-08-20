# Claim/Lease Protocol

Multiple agents on different machines (engineers' laptops, remote runners)
must never double-work an item. GitHub is the lock service; no other
infrastructure exists. The protocol is deliberately tolerant of crashed or
disconnected workers.

## Claiming

To claim an issue in `factory:ready`:

1. **Check**: the issue has no assignee and no unexpired lease comment.
2. **Claim atomically enough**: self-assign the issue, then post the lease
   comment within the same minute. If, after posting, a *different* worker's
   lease comment has an earlier timestamp, back off: remove your assignment
   and your lease comment.
3. **Relabel**: `factory:ready` → `factory:claimed`.

## Lease comment format

A lease is a PR/issue comment whose body is exactly one fenced YAML block:

```yaml
factory-lease:
  worker: "<harness>/<host-or-handle>"     # e.g. "pi/ysh-mbp", "claude-code/runner-3"
  claimed_at: "2026-08-18T09:00:00Z"       # ISO-8601 UTC
  expires_at: "2026-08-18T13:00:00Z"       # claimed_at + TTL
  branch: "factory/35-claim-protocol"      # branch the worker will push
  renewal: 0                               # increments on each renewal
```

- **Default TTL: 4 hours.** Size class L may use 8 hours.
- **Renewal**: post a new lease comment with `renewal: n+1` *before* expiry.
  A renewal without visible progress (no new commits on the branch since the
  previous lease) is invalid — the item is reclaimable.
- **Clock discipline**: all timestamps UTC; workers must tolerate ±5 minutes
  of skew before calling a lease expired.

## Reclaiming

Any worker finding a `factory:claimed`/`factory:in-progress` item whose
newest lease is expired may reclaim it:

1. Post a reclaim comment naming the expired lease and its worker.
2. Remove the stale assignee, assign self, post a fresh lease.
3. Reference the previous branch if one exists; the reclaimer decides whether
   to continue it or restart, and says which in the reclaim comment.

Never reclaim an unexpired lease, and never delete another worker's branch.

## Idempotency rules

- All factory operations must be safe to repeat: relabeling to the current
  label, re-posting an identical lease, re-opening an existing draft PR are
  no-ops, not errors.
- A worker that crashes mid-claim leaves either (a) an assignee without a
  lease — treated as no claim after 10 minutes — or (b) a lease without a
  branch — expires naturally.
- One worker holds at most **two** concurrent leases across the repository.

## Worker capability descriptors

Some verification commands need host capabilities (Nix devshell, iOS
simulator, Android emulator). A worker only claims items whose verification
commands it can actually run; the work-item template records the required
capabilities so this is checkable before claiming.
