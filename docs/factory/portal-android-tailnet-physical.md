# Portal physical Android Tailnet lane

## Purpose and boundary

This owner-invoked L4 lane runs the Lace ID Portal conformance journey on one
physical Android phone through a temporary Tailscale HTTPS Serve profile. It is
development evidence, distinct from the loopback macOS and virtual-mobile lanes;
it is not production trust/discovery, live KYC, native-custody, release, or
simulator evidence. [ADR-0103](../adr/0103-compose-portal-final-for-physical-android.md)
is the implementation authority.

## Preconditions and execution

Start from a clean committed candidate and a healthy standalone stack on 6300,
8088, and 9944. Missing listeners are normally actionable: inspect their owner,
start Docker Desktop only with authorization, then run `just standalone-up`.
Do not recreate a healthy pre-existing stack without its owner's authorization.

Tailscale must be online on the Mac and phone. Allow exactly one reviewed,
connected non-QEMU ADB device; disconnect emulators and never use a simulator
as physical evidence. The harness discovers its private Tailscale identity and
an unused HTTPS listener, validates the existing Serve baseline without
printing identities, and restores the exact Serve state through its receipt.
Run:

```bash
just android-portal-tailnet-physical-smoke
```

## Owner manual QR demonstration

This optional lifecycle is a live owner demo, not physical-lane evidence and
never a replacement for the automated physical or simulator lanes. Prepare the
exact pinned Portal artifacts first. This phase does not require a phone,
Tailscale, or the standalone stack and may be safely rerun after a failure.

The first ADR-0117 Factory-flow canary makes this preparation sequence
reviewable before it runs. The commands below resolve only the exact pinned
Taskflow core and spend zero model tokens:

```bash
just taskflow-portal-tailnet-verify
just taskflow-portal-tailnet-plan
just taskflow-portal-tailnet-compile
```

The plan is `preflight → prepare-artifacts → verify-prepared-artifacts →
handoff`. Taskflow execution remains disabled while issue #690 proves
long-process progress, cancellation, resume, and orphan cleanup. To exercise the
prototype interactively, open the saved flow in Pi after that conformance gate;
until then, follow the same explicit checkpoint below. Rejecting its approval is
supposed to halt the flow—there is no bypass around a failed preparation.

```bash
just portal-tailnet-manual-prepare
just portal-tailnet-manual-prepared-status
```

Preparation realizes and loads the resolver, DID manager, and issuer images one
at a time and pulls the pinned Smocker support image before the interactive
window. Each completed Portal image is checkpointed with its immutable image ID,
independent archive digest, Nix output path and persistent GC root, cache-hit
marker, and elapsed seconds. A retry resumes after
the last validated checkpoint instead of rebuilding successful phases.

Only one preparation process may own the receipt. `preparation-busy` means an
existing preparer still owns the lock; wait for it to finish. If start reports
`artifacts-not-prepared`, rerun `just portal-tailnet-manual-prepare` and then
`just portal-tailnet-manual-prepared-status` before reconnecting the phone.

With the preparation receipt complete, connect the phone and Tailscale, ensure
the standalone stack is healthy on 6300, 8088, and 9944, and start one session:

```bash
just portal-tailnet-manual-start
```

Start fails before changing Tailnet or phone state if the prepared source,
receipt, Nix outputs, or loaded Docker image IDs no longer match. It creates and
validates the same private mode-`0600` pinned mock transform used by the browser
contract, then exposes its KYC page under the receipt-owned same-origin HTTPS
`/kyc` mount. It opens the Portal page in the Mac browser and prints the one
permitted public page URL plus service, Tailnet, Android, and total readiness
timings; status intentionally reveals no payload. This owner demo remains
non-evidence. Start is the foreground owner and deliberately remains running;
do not close that terminal. It reports `READY` only after two independent
receipt, process, Docker, device, Serve, and public-page checks. Run status or
stop from another terminal:

```bash
just portal-tailnet-manual-status
```

On the phone, explicitly prepare the holder before accepting an offer:

1. Open **Wallet** and activate the development wallet if it is not active.
2. Open **Documents** → **Manage identities** and create a standalone DID.
3. Tap **Publish active holder DID to test issuer** and wait for the confirmation
   that the public DID document is available. This shares only the public DID
   Resolution Result with this receipt-owned test issuer; it sends no private
   keys or credentials and is not a Midnight on-chain DID publication.
4. Complete the Portal page, use Oxid's **Scan** action to scan its QR once,
   preview the offer, then choose **Accept and issue credential** or
   **Refuse offer**.

After successful acceptance, the offer preview closes and a short **Saved to
your wallet** receipt appears above the protected inventory. The same action
already performed issuance, verification, and encrypted persistence; there is
no second receive action. The UI does not expose the fixture-ingestion
capability because it bypasses OpenID4VCI. A Digital Passport card lists the
validated first name, last name, date-of-birth predicate, optional document
number, and issuing state capabilities. Selective attributes remain encrypted
until the holder taps **Reveal locally**; the date of birth remains
predicate-only.

Do not retry or reuse a consumed QR. Stop before a fresh attempt:

```bash
just portal-tailnet-manual-stop
```

Stop validates the session/process/Serve receipts, removes only owned Portal
runtime state, and restores the exact prior Serve baseline. It retains prepared
artifacts so the next start avoids a build-from-scratch. If a receipt is
ambiguous, it fails closed for owner review rather than deleting shared state.
An interrupted Portal Compose startup leaves a private provisional ownership
receipt, so the exact project can be recovered with the same stop/cleanup path
instead of becoming an unowned partial stack.

Manual start installs a compatible APK with Android's data-preserving upgrade
path and does **not** clear profiles, custody associations, credentials,
preferences, or diagnostics. If the owner explicitly needs an empty Oxid data
container, stop the session first and invoke the destructive operation by name:

```bash
just portal-tailnet-manual-reset
```

Reset prints the exact package and `application-data` scope before mutation,
refuses to run while a manual session is active, preserves the installed APK,
and verifies that the package still exists. The automated physical conformance
lane remains clean-room evidence and therefore selects app-data reset explicitly
inside its owned disposable run.

## Safety, evidence, and cleanup

Every retry uses a fresh offer, capability, app state, and runtime. The holder
bootstrap is an explicit app action; the physical lane must never scrape or
publish the wallet DID store through ADB. Never reuse a consumed offer. Require
refusal with zero secret endpoint calls before explicit consent, encrypted
persistence, a real process restart, listing, and a fresh reverification.

The command publishes only redacted mode-`0600` evidence after exact cleanup;
evidence must name the current Oxid `HEAD` and tree. It excludes device and
tailnet identities, endpoints, offers, capabilities, credentials, and protocol
secrets. Preserve ambiguous state for review. Remove only receipt-proven
Portal/process resources and a standalone stack this session owns or whose
owner expressly authorizes teardown.
