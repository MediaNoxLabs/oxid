# ADR-0117: Compose Factory flows with pi-taskflow

- Status: Accepted for bounded adoption
- Date: 2026-09-23
- Issue: [#689](https://github.com/MediaNoxLabs/oxid/issues/689)
- Depends on: ADR-0094 and the phased Portal lifecycle from #687/#688
- Follow-up: [#690](https://github.com/MediaNoxLabs/oxid/issues/690) Taskflow long-process conformance canary

## Context

The Factory already owns exact Nix environments, reviewed shell and MJS
entrypoints, a demo inventory, Pi skills, local gate receipts, and CI target
selection. What it lacks is one durable representation of how those operations
compose. A human, Codex, and Pi can therefore prepare the same demonstration in
different orders and with different recovery or cleanup behavior.

The desired unit is a reusable flow: typed inputs select a fixed DAG of build,
copy, setup, deploy, readiness, handoff, and cleanup steps. Preconditions must
fail before effects. Ongoing readiness and final acceptance must be observable.
The supervisor must inspect the plan before model, compute, device, or network
spend and later resume only the unfinished or stale frontier.

`pi-taskflow@0.2.10` already supplies saved DAGs, typed arguments, structural
verification, a zero-token plan, compilation to reviewable FlowIR/Mermaid,
phase dependencies, approvals, gates, budgets, persistence, resume, replay,
incremental recomputation, and analytics. Reimplementing those semantics in an
Oxid MJS or Python scheduler would create two competing workflow authorities.

The previous detached-runner experiment nevertheless exposed real admission
failures: dependency closure mismatch, invisible nested progress, idle timeout,
and orphan cleanup. Script phases are also capped at 300 seconds, while a cold
Nix, Docker, Xcode, or Android build can legitimately take longer. Those are
runtime-conformance gaps, not reasons to duplicate Taskflow's compiler.

## Decision

Taskflow is the canonical Factory flow definition, compiler, DAG state machine,
and future run history. Repository-owned scripts remain the only mechanical
effectors. Nix pins the tools, build inputs, packages, and cacheable artifacts.
`just` remains the concise human interface. Skills and agents select, plan, and
supervise reviewed flows rather than composing arbitrary shell programs.

The boundaries are:

| Layer | Authority |
| --- | --- |
| Taskflow | Typed arguments, DAG edges, conditional control, approvals, gates, budgets, resume/replay, and agent work |
| Factory adapter | Exact package resolution, static verify/plan/compile, future secret-safe receipts, resource locks, and effect ownership |
| Nix | Reproducible tools, source inputs, derivations, applications, and shared binary caches |
| Shell/MJS entrypoints | One bounded operation with explicit exit status and payload-safe output |
| Just | Human-readable aliases for the same repository entrypoints |
| CI | Invokes the same static contracts and, after conformance, selected non-interactive flows |

No second graph scheduler, workflow database, server, or dynamic shell generator
is introduced. Normal structured inputs belong in typed Taskflow arguments or
private mode-0600 files. Environment variables are limited to process context
and secret handles; secrets, endpoints, credentials, and transcripts never enter
committed definitions or public receipts.

### Admission stages

Adoption is intentionally staged:

1. The pinned core is exposed through `scripts/factory/taskflow-static.mjs` for
   `verify`, `plan`, and `compile` only. These operations make no model calls and
   execute no flow phases.
2. A repository-owned saved flow models Portal Tailnet artifact preparation.
   Its long build remains an explicit foreground approval boundary; the existing
   resumable lifecycle owns the build and private receipt. Taskflow verifies the
   exact prepared receipt afterward.
3. Issue #690 must prove long-running progress, process-tree
   cancellation, no orphans, restart/resume, cache invalidation, and cleanup.
4. Only after that canary passes may the Pi Taskflow extension execute mutating
   mechanical phases. The approval boundary can then become a supervised script
   phase without changing the surrounding flow contract.
5. Agent phases are admitted after the deterministic execution path is stable.

The project Pi settings therefore continue to suppress inherited Taskflow
extensions and skills during stage 1. The static adapter resolves exactly the
repository-pinned `pi-taskflow` and matching `taskflow-core`; it fails closed on
a missing or mismatched closure.

### First flow

`.pi/taskflows/flows/demos/portal-tailnet-prepare.json` models:

```text
preflight
    -> prepare-artifacts (foreground human checkpoint around the long command)
    -> verify-prepared-artifacts (exact source/image receipt)
    -> handoff (environment prepared; no device or Tailnet mutation yet)
```

The first canary requires neither a phone nor Tailscale. It does not start the
issuer, publish routes, build a mobile target, or claim an end-to-end issuance.
Those effects remain in the existing owner-gated lifecycle and become later
flows after the runtime conformance gate.

## Consequences

- Factory plans become versioned, diffable data and can be inspected without
  token spend.
- Pi, Codex, humans, and CI share one flow vocabulary while retaining the same
  reviewed mechanical entrypoints.
- The first slice is useful even before mutating execution: it makes the long
  preparation checkpoint, validation, and handoff deterministic.
- Taskflow limitations stay visible as one bounded follow-up rather than being
  hidden behind an agent waiting on a terminal or an unowned background process.
- The flow library can later compose preparation, service readiness, Tailnet
  publication, target build/deploy, manual demonstration, and exact cleanup.
- File receipts remain sufficient initially. A database is not introduced until
  concurrent local/cloud supervisors demonstrate a query or lease requirement.

## Alternatives rejected

- A new MJS or Python DAG runner would duplicate verification, persistence,
  caching, replay, and agent orchestration while creating a migration burden.
- Encoding the process only in Nix would mix pure build composition with host,
  device, Docker, and Tailnet side effects.
- Asking an agent to run and watch arbitrary shell commands makes cancellation,
  ownership, inputs, and metrics prompt conventions rather than runtime rules.
- Enabling all Taskflow mutations immediately would ignore the measured
  detached-runner and long-script failures.
- Temporal, Argo, Tekton, and similar service schedulers add operational
  infrastructure without solving local Xcode, device, and Tailnet ownership.
