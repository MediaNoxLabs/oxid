# Portal macOS laptop lane

## Purpose and applicability

Use this owner-invoked L4 lane to prequalify a committed Oxid candidate on an
Apple-silicon development Mac. It runs the existing headless Portal journey
first to localize protocol/composition failures, then the native Dioxus journey
to exercise the same shared behavior before entering mobile-specific lanes.
The aggregate command is not a `HostedTarget`, CI gate, release lane, or claim
that the combined run is faster overall; its duration is unmeasured. After this
lane passes, the [Portal mobile simulator lane](portal-mobile-simulators.md)
provides the separate packaged iOS Simulator and Android QEMU continuation.

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

Before starting, run this complete parenthesized block in Bash. It grants
cleanup authority only from a successful, process-local Docker baseline:

```bash
(
  set -e
  test -z "$(git status --porcelain --untracked-files=no)"
  if ! standalone_before="$(docker ps -a \
    --filter label=com.docker.compose.project=oxid-standalone \
    --format '{{.ID}}' 2>/dev/null)"; then
    printf '%s\n' 'standalone ownership query failed; no cleanup authority installed and no stack command run' >&2
    exit 1
  fi

  standalone_owned=false
  if [ -z "$standalone_before" ]; then
    standalone_owned=true
  fi

  cleanup_owned_standalone_on_failure() {
    failure_status=$?
    trap - EXIT
    if [ "$failure_status" -ne 0 ] && [ "$standalone_owned" = true ]; then
      if just standalone-down; then
        :
      else
        cleanup_status=$?
        printf 'owned standalone cleanup failed (exit %s); preserving stack state for owner review; no force deletion attempted\n' \
          "$cleanup_status" >&2
      fi
    fi
    exit "$failure_status"
  }
  trap cleanup_owned_standalone_on_failure EXIT

  just standalone-up
  just portal-macos-laptop-e2e
  jq -s -e \
    --arg head "$(git rev-parse HEAD)" \
    --arg tree "$(git rev-parse 'HEAD^{tree}')" \
    'length == 2 and all(.[]; .oxid == {head:$head,tree:$tree})' \
    target/portal-headless-e2e/evidence.json \
    target/portal-desktop-e2e/evidence.json
  trap - EXIT
)
```

Run the block as one invocation, not as separate commands. A failed Docker
query exits before the local ownership boolean and failure trap exist.
`just standalone-up` must validate exactly three healthy standalone services
whether the stack is new or pre-existing. `just portal-macos-laptop-e2e`
accepts no arguments, runs headless then desktop exactly once, stops at the
first failure, and performs the same slurped aggregate check. Its final success
line is:

```text
portal-macos-laptop-e2e: PASS evidence=target/portal-headless-e2e/evidence.json,target/portal-desktop-e2e/evidence.json
```

If the successful baseline was empty, only this Bash process may treat the
stack as owned. A later failure invokes `just standalone-down` through the EXIT
trap; if cleanup fails, the trap reports it, preserves the original failure,
and does not force-delete anything. A nonempty baseline never authorizes
standalone cleanup. On success, the block disarms the trap and leaves a stack
started by this invocation running for deliberate inner-loop reuse.

Any legacy `tmp/portal-macos-laptop/ownership.txt` files are untrusted
historical state. This method never reads, writes, or removes them, and they
never authorize cleanup.

Harness cleanup is receipt-scoped to `oxid-portal-consumer`; it never prunes
Docker or removes `oxid-standalone`. If a receipt or lock cannot prove ownership
and restoration, preserve the containers, state, and lock for owner review.
Report cleanup failures and never force-delete containers, state, or locks.

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
