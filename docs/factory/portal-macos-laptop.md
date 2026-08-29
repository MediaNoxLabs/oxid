# Portal macOS laptop lane

## Purpose and applicability

Use this owner-invoked L4 lane to prequalify a committed Oxid candidate on an
Apple-silicon development Mac. It runs the existing headless Portal journey
first to localize protocol/composition failures, then the native Dioxus journey
to exercise the same shared behavior before entering mobile-specific lanes.
The aggregate command is not a `HostedTarget`, CI gate, release lane, or claim
that the combined run is faster overall; its duration is unmeasured.

## Prerequisites

- Apple-silicon macOS in an active WindowServer session.
- Xcode at `/Applications/Xcode.app` with a usable macOS SDK.
- The Nix development shell, Docker Desktop running, Git/network access, and
  the tools checked by the harnesses (including Cargo, Node, `jq`,
  `screencapture`, and `shasum`).
- A tracked-clean, committed candidate `HEAD`.
- No `oxid-portal-consumer` containers and no unresolved
  `target/portal-virtual-mobile/stack.lock`. Ports used by standalone
  (6300, 8088, 9944), the Portal consumer (8081, 8090, 8098, 9090, 9092), and
  the desktop stack (18090, 18091, 18093, 18095) must be free except for ports
  held by the validated pre-existing `oxid-standalone` stack.
- Screen Recording permission for the terminal or application that launches
  the command. It is used only for protocol-redacted native consent and restart
  window crops. It does not drive the UI, grant protocol authority, or replace
  assertions. The compile-gated rendered-control driver requires neither
  Accessibility nor System Events. Missing permission must fail screenshot
  evidence; never bypass that gate.

## Owner-safe execution

Before starting, record whether any standalone containers already exist:

```bash
mkdir -p tmp/portal-macos-laptop
if ! standalone_before="$(docker ps -a \
  --filter label=com.docker.compose.project=oxid-standalone \
  --format '{{.ID}}' 2>/dev/null)"; then
  printf '%s\n' 'standalone ownership query failed; no ownership recorded and no stack command run' >&2
  exit 1
fi
printf 'standalone_preexisting=%s\n' "$([ -n "$standalone_before" ] && echo true || echo false)" \
  > tmp/portal-macos-laptop/ownership.txt
test -z "$(git status --porcelain --untracked-files=no)"
just standalone-up
just portal-macos-laptop-e2e
jq -s -e \
  --arg head "$(git rev-parse HEAD)" \
  --arg tree "$(git rev-parse 'HEAD^{tree}')" \
  'length == 2 and all(.[]; .oxid == {head:$head,tree:$tree})' \
  target/portal-headless-e2e/evidence.json \
  target/portal-desktop-e2e/evidence.json
```

Run those commands in that order. `just standalone-up` must validate exactly
three healthy standalone services whether the stack is new or pre-existing.
`just portal-macos-laptop-e2e` accepts no arguments, runs headless then desktop
exactly once, stops at the first failure, and performs the same slurped
aggregate check. Its final success line is:

```text
portal-macos-laptop-e2e: PASS evidence=target/portal-headless-e2e/evidence.json,target/portal-desktop-e2e/evidence.json
```

If no standalone containers existed before `just standalone-up`, this session
owns the stack. On success, leave an owned stack running for deliberate
inner-loop reuse; run `just standalone-down` later only for that owned stack.
On failure, run `just standalone-down` only when this session owns it. Never
remove a pre-existing stack.

Harness cleanup is receipt-scoped to `oxid-portal-consumer`; it never prunes
Docker or removes `oxid-standalone`. If a receipt or lock cannot prove ownership
and restoration, preserve the containers, state, and lock for owner review.
Never force-delete them.

## Pass evidence and exact-head rule

A pass requires both harnesses to return zero in the stated order, both JSON
files to exist and pass their individual schemas, and the slurped array to have
length two with every `.oxid` equal to current `HEAD` and `HEAD^{tree}`. The
desktop consent and restart PNGs must pass the existing format, size, and
redaction checks.

Evidence is ignored and remains local:

- `target/portal-headless-e2e/evidence.json`
- `target/portal-desktop-e2e/evidence.json`
- `target/portal-desktop-e2e/screenshots/consent.png`
- `target/portal-desktop-e2e/screenshots/restart.png`

Any commit or tracked edit after the run makes all aggregate evidence stale;
restore a clean committed candidate and rerun the complete command. Node and
proof-server interactions remain explicitly unproven.

## Scenario boundary

This lane proves the pinned Lace `integration@22ae5369…` Rust issuer, resolver,
and did-manager in supported Smocker Didit mode (not an external KYC provider);
exact offer routing; refusal before consent; explicit acceptance; managed
authentication; separate Jubjub holder binding; Digital Passport verification;
encrypted persistence, listing, restart restoration, and fresh reverification;
and headless/native Dioxus observation of local indexer synchronization.

It does **not** prove Android/iOS compilation, deployment, simulator/emulator or
physical-device behavior; camera, custody, resource, or Tailscale behavior;
production trust/discovery, live DIDIT, real-person KYC, release readiness, or
direct Oxid use of the node/proof server. It does not promote this lane into
`HostedTarget` or required CI, rewrite harness/product/protocol logic, measure
an overall speedup, resolve the remaining issue #2 backlog or ADR drift, or
modify the installed pinned `dev-loops` package.
