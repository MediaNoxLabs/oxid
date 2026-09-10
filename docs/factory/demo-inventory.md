# Demo inventory

[`demo-inventory.json`](demo-inventory.json) is the single machine-readable source
for product demonstration truth. It links atomic **use cases** to ordered
**scenarios**, then scenarios to product/audience **demos**. IDs, rather than
copied runbook prose, carry every relationship. Its closed structure is
documented by [`demo-inventory.schema.json`](demo-inventory.schema.json), while
the repository validator enforces cross-reference and safe-command semantics.

Use the read-only renderer; it validates before rendering and never evaluates
inventory command text:

```bash
node scripts/demo-inventory.mjs check
node scripts/demo-inventory.mjs list
node scripts/demo-inventory.mjs show wallet-root-recovery-native-presence
node scripts/demo-inventory.mjs prepare wallet-root-recovery-native-presence android-physical
node scripts/demo-inventory.mjs use-case list
```

In Pi, `/scenario list`, `/scenario show <id>`, and
`/scenario prepare <id> [target-id]` use the same validator; `/use-case list`
and `/use-case show <id>` expose the atomic aliases. When no target is supplied,
`prepare` uses the scenario's reviewed default target.

The repository CLI only renders a brief and never evaluates inventory command
text. Invoking Pi's `/scenario prepare` is the user's request for the active
agent to perform bounded preparation: load resource-hygiene, select one
supported target, check prerequisites, start only authorized dependencies,
build/deploy or use the one-command run path, prove readiness, and return
non-sensitive URLs plus manual acceptance and cleanup steps. It does not
authorize entering credentials, completing consent, deleting pre-existing
state, or exceeding `AGENT.md`. Commands marked `unsupported`, `manual`, or
`delegated` describe an honest boundary instead of inventing automation.

Each target plan exposes separate `build`, `deploy`, and `run` operations. The
agent chooses the all-in-one `run` path or the reusable build/deploy path; it
does not execute both redundantly.

## Maintenance contract

The Product Manager assesses every shipped capability for a use case. If it has
a demonstrable journey, add it to an ordered scenario and a product demo; if it
does not, record the justified no-demo impact in feature closeout. Keep target
support, dependency ownership, health checks, cleanup, evidence class, cadence,
test mapping, manual acceptance, and expected outcomes explicit. Reuse the
linked scripts/runbooks rather than copying their operational detail.

Validation fails closed for duplicate or unknown IDs, missing source references,
unsafe or absolute commands, mutable dependencies without cleanup, unsupported
cadence/evidence values, and scenarios without test mappings. Live, expensive
or physical evidence remains on its declared cadence; it is not a universal PR
gate.

## Initial slice

`wallet-root-recovery-native-presence` records the merged native-presence recovery
behavior. Desktop is only a fast development preflight; simulator diagnostics
must not be promoted to acceptance. Physical Android is authoritative manual
acceptance and uses the existing native-custody build/deploy/run launcher.
Physical iOS deployment remains explicitly unsupported. The scenario covers
denial, one-shot root installation, duplicate rejection, and lifecycle clearing;
automated, privacy-preserving physical-device evidence remains a planned gap.

## Standalone asset-sync slice

`standalone-profile-asset-synchronization` covers only the shipped explicit
standalone-development composition: its fixed `undeployed` network identity,
compile-time `local` (loopback) or `tailnet` route class, and independent
public NIGHT, DUST, and shielded synchronization states. Local simulator runs
are diagnostic. The physical Android Tailnet lane is also a development-route
diagnostic, not public-network or production acceptance. It records freshness
states rather than a balance assertion because the public-genesis development
state is mutable.

## Development proof and local diagnostics slice

`development-proof-benchmark-desktop` is an on-demand desktop diagnostic that
runs exactly one operator-selected k=1 proof. Its explicit development build
may fetch public proving parameters into an app-private temporary cache. The
operator owns network approval, disk budget, and cache retention. The native
panel permits ordinary k=1–17 controls, but this bounded scenario does not run
a sweep. k=18–21 remain a separate visible-consent resource risk requiring an
owner-invoked resource receipt. Public RSS/CPU is truthfully unavailable because
no reviewed public sampler exists. A verified process-local timing report is
diagnostic evidence, not a performance baseline, release gate, or acceptance.

`bounded-local-diagnostics-headless` deterministically demonstrates the closed
snapshot and exact-confirmation clear contract. `bounded-local-diagnostics-desktop`
separately demonstrates the rendered counts, newest-first rows, Warning/Error
filters, fixed-code search, and cancel/confirm clear controls; an empty ring is
a valid state. Both expose diagnostic support information, not acceptance
evidence. Events are bounded, payload-free, fixed-code, explicitly clearable,
and process-local. They are neither persisted nor uploaded, do not receive
benchmark telemetry, and must never be represented as a durable support journal.

No simulator or physical-device target is claimed for this slice. Desktop and
headless diagnostics do not establish native custody, device resources, release
readiness, or physical acceptance.

## Holder-DID bootstrap boundary

The physical Android Tailnet Portal scenario composes user-visible creation of
one managed undeployed holder DID with the separate explicit action that makes
only its public resolution result available to the receipt-scoped test issuer.
It does not claim a separate DID demo, Midnight on-chain publication,
production discovery, native-custody acceptance, or durable public acceptance.
The existing optional manual Portal lifecycle is non-evidence and retains its
paired stop boundary.

There is no inventory demo for generic standalone DID resolve, sign, update, or
deactivate: their headless lifecycle contract is in-memory, while the broad
Android/iOS profile smokes combine unrelated simulated journeys and do not have
a separate receipt-scoped public demonstration. Those remain follow-up gaps,
not acceptance evidence; milestone acceptance remains tracked by issue #291.

## Portal Final issuance slice

`portal-final-digital-passport-issuance` records the complete user-visible
Portal Digital Passport journey: explicit offer review and consent, managed DID
authentication, separate Jubjub binding, strict verification, encrypted storage,
restart, listing, and fresh reverification. Its evidence is deliberately split:
controlled localhost headless/native desktop is **preflight**, packaged iOS
Simulator and Android QEMU are **diagnostic**, and physical Android Tailnet is
also a **development diagnostic**. None is production trust/discovery, live KYC,
native-custody, release, public-network, node, or proof-server acceptance.

The mocked-Smocker headless run supports the desktop UI preflight but never
substitutes for a rendered journey. Virtual targets never substitute for the
physical Android lane. The optional physical browser/QR lifecycle remains an
owner demo, not evidence. All reuse their existing receipt-scoped runbooks and
cleanup; a pre-existing standalone baseline remains with its owner.

### Credential-protocol no-demo dispositions

- The deterministic standalone OID4VCI API is shipped and covered by headless
  and mobile contracts, but has no separate receipt-scoped user-visible runbook;
  it is not a demo apart from the Portal Final journey.
- SIOPv2 DID authentication is shipped with deterministic headless/mobile flows,
  but lacks a dedicated hygienic scenario/runbook and target-specific evidence;
  no demo is claimed ahead of issue #291's milestone acceptance work.
- Default OpenID4VP prepares matching and consent but fails closed at
  `proof_unavailable`; its partial API is not a presentation demo. The explicit
  headless/native-proving path and experimental mobile proving composition lack
  a reusable receipt-scoped user-visible runbook, while physical resource and
  native-custody acceptance remain open under issues #27 and #291.
- The historical incompatible Portal fixtures are negative parser regression
  evidence only, not positive issuance evidence.
