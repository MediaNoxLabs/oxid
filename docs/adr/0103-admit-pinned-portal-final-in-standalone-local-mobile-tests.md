# ADR-0103: Admit pinned Portal Final in standalone-local mobile tests

- Status: Accepted
- Date: 2026-08-21
- Source: [issue #124](https://github.com/MediaNoxLabs/oxid/issues/124)
- Portal integration source: squash commit `925ec8d04882eabd4ac7b784c70fc2f0c152faae`, tree `58b4597524f88a0ae2253439a44dab0dc60cbb6f`
- Portal lifecycle helper: signed commit `f7732be01171cf6a376ec0dd043f517e3f6fcf6b`, tree `96accf0da80992c3b247458c3b21f22ee9db1d68` (Portal PR #19 remains draft and human-merge-only)
- Historical Portal PR head: `9c82db23eabe8b6d758b2731f2225910ea627c14`
- Profile source: `76e8edf394a4cb37ca822037272d543c68f25f71`; provenance SHA-256 `cf86f4ddb06131d7570c835e8c6c62d524e8179fe6a53436b20d2d4e72b44d87`
- Amends: ADR-0039
- Extends: ADR-0097, ADR-0101, and ADR-0102
- Implementation state: explicit iOS Simulator and Android QEMU `standalone-local` Portal test profiles, sequential local repository harnesses, same-head platform-plus-standard-smoke evidence, and secret-free retained records are implemented; hosted CI validates only repository-owned static/contract boundaries and receives no private Portal credential; production and ordinary-mobile transport, native custody, tailnet Portal, physical-device, real-camera, live-holder-DID, production-trust, WebAssembly, and unsupported-grant paths remain unavailable

## Context

ADR-0102 admitted the landed strict Portal profile only to native
headless/desktop development. The mobile suites still used the deterministic
embedded issuer, so they proved the same Oxid use cases but not Portal HTTP in
the iOS and Android test frameworks. Issue #124 requires that distinction to be
closed without turning a simulator route into production configuration.

The real offer contains a single-use pre-authorized grant. A normal shell trace,
rendered textarea, evidence file, or diagnostic body must not reproduce it.
Android also cannot use `10.0.2.2`: ADR-0027 permits plaintext proving and this
Portal profile only at syntactic loopback, so the emulator must use exact
repository-owned reverse mappings.

## Decision

Add the application feature `standalone-portal`. It implies `mobile`,
`standalone-development`, `standalone-local`, and the composition-owned mobile
Portal HTTP feature. Feature selection alone is insufficient: `build.rs`
requires an authenticated, exact transient profile-authority manifest emitted
by the repository launcher. iOS permits only simulator target triples; Android
emits authority only after a live `ro.kernel.qemu=1` check and the app repeats a
payload-free `Build`-fact QEMU check before constructing Portal composition.
`oxid-app` also rejects every native-custody or tailnet combination. The feature
is not a public runtime setting.

The launcher accepts `OXID_MOBILE_PORTAL_PROFILE=local` only with development
custody and the local route profile. It authenticates one absolute regular
non-symlink deployment manifest and lowercase SHA-256, then the build script
embeds those exact bytes into the selected app artifact. Mobile composition
constructs the existing strict Portal client from those bytes and combines it
with the existing standalone-local Midnight routes. No app runtime environment,
link, UI input, forwarded host, or metadata response can select another route.
Normal `compose()`, native-custody composition, tailnet composition, and
WebAssembly still cannot select or name this constructor.

The Portal client and all existing authority boundaries remain unchanged:

- by-value Final offer and separate issuer/OAuth metadata only;
- no Transaction Code, legacy decoder, redirect, proxy, retry, cookie, or replay fallback;
- loopback-only HTTP and HTTPS for every nonloopback origin;
- explicit WHO/WHAT/FROM/WHY preview and literal consent;
- current managed authentication proof plus a distinct managed Jubjub assertion method;
- exact body, detached proof, private-material conversion, issuer/time/trust verification, valid-only import, and encrypted persistence.

An OS-delivered offer is retained transiently for prepare but is never rendered
as an editable DOM value. It is cleared as soon as the protocol adapter accepts
it into its private prepared session. Manual deterministic development input
remains editable outside this imported-link case.

## Test harness

`just portal-local-conformance <profile>` is the fixed complete local recipe:
the real headless boundary completes first, iOS Portal conformance and the
standard iOS smoke complete next, and Android Portal conformance plus the
standard Android smoke complete last. The focused platform commands require the
same explicit owner-private v1 profile and remain diagnostic entry points only.

The complete recipe starts or non-mutatingly attaches the Oxid-owned
`oxid-standalone` project, delegates the separate Portal-only lifecycle to the
signed `f7732be...` helper, and then runs every consumer against those same
projects. Portal owns smocker, resolver, did-manager/bootstrap and issuer only;
its reviewed integrated Compose declares no node, indexer, or proof service.
Each platform command:

1. authenticates the helper commit/tree separately from the detached protocol
   commit/tree, historical PR head, profile source and provenance digest;
2. uses the profile's persistent clean detached `925ec8d...` protocol source
   without fetching or creating a temporary worktree;
3. attaches to the already-ready Portal project and never invokes its Compose
   up/down/recreate operations;
4. runs fixed loopback proxy/resolver endpoints while the issuer retains the
   profile-selected public origins;
5. derives the canonical secret-free deployment manifest and creates a fresh
   approved mock-KYC offer for each fixed-trigger handoff;
6. permits exactly one capability-authenticated response and zeroizes the
   in-memory handoff;
7. drives the existing strict ingress, consent, exchange, verification,
   encrypted-store and restart/reverification path; and
8. removes only platform-private runtime, support processes, locks and dynamic
   device forwarding. Owner lifecycle cleanup remains with
   `local-headless-down` after every consumer has finished.

A profile-scoped Midnight owner receipt is stored only in the external private
state directory and only when that invocation created the exact three-container
project. An attach or consumer shutdown cannot call Midnight Compose down or
Tailscale reset. Portal shutdown verifies shared container identity and
non-decreasing height while removing only its exact project. Cleanup uncertainty
is a failure, not authority to prune by prefix or delete unrelated resources.

Neither `simctl openurl` nor `am start -d` receives the offer. Both deliver
only a fixed, non-secret trigger URL under the existing
`openid-credential-offer` scheme
(`openid-credential-offer://standalone-portal-test-fetch`). The app, built with
the compile-time `loopback-test-offer-trigger` feature that
`standalone-portal` selects, recognizes only that exact literal. Before each
trigger, the harness provisions a fresh 256-bit capability without placing it
in argv or retained output: iOS writes directly to the simulator app data
container, while Android streams it on stdin through `run-as`. The named worker
requires an owner-private fixed file, unlinks it before use, sends the
capability only in an Authorization header, and zeroizes its buffer. The
loopback endpoint changes state before writing any offer byte, so exactly one
concurrent caller succeeds and replay fails. The offer response is time- and
size-bounded before entering the existing one-item router; Tao/Wry's OS callback
never blocks on retrieval. No real offer, grant, or capability enters
host/device argv, OS URL/intent state, logs, or retained evidence. A failed
fetch enqueues the inert trigger string, which the strict
`openid-credential-offer` route rejects as malformed rather than ever treating
it as a real grant. The QEMU gate cold-reboots an
already-running disposable emulator so its system clock stays within two
seconds of the host; strict credential temporal verification is not weakened
with future-time slack. It then verifies exact reverse entries for 8088, 9944,
6300, Portal 18090, fixed-trigger control 18091, and resolver 18093. It never
installs `10.0.2.2` and never uses `reverse --remove-all`.

The suites collectively cover warm/cold delivery, one review item, refusal,
malformed input, unavailable transport, bounded timeout, explicit consent, success, process
restart, truthful development-custody reset/reactivation, encrypted list, and
reverification. iOS also proves simulator camera unavailability. The existing
Android ingress suite remains the authority for scanner cancellation, timeout,
and vendor unavailability.

## Evidence and consequences

The complete recipe stages headless, iOS, and Android evidence away from the
retained paths while every real and standard smoke runs. It adds the closed
`standardSmoke` acceptance boolean to each platform candidate only after the
matching standard smoke succeeds. One final validator then requires all three
documents to bind the exact same Oxid head and canonical Portal helper commit/tree plus protocol commit/tree,
historical PR head, profile source, and provenance digest. It also requires the
exact headless/iOS/Android schemas, platform identities, every acceptance
boolean, and the secret sentinel before publication.

The retained files are:

- `target/portal-headless-e2e/evidence.json`;
- `target/portal-mobile-e2e/ios/evidence.json`;
- `target/portal-mobile-e2e/android/evidence.json`.

They are ignored, closed local review inputs. Platform evidence may record a
virtual model, OS/API, application id, and fixed reverse ports. It excludes
simulator/emulator identifiers, routes, DIDs, grants, tokens, nonces, JWTs,
credential bytes, proofs, private parts, claims, logs, PIDs, and timestamps; a
sentinel scan rejects common representations. The selected virtual device's
installed Oxid app/data and normal build outputs remain available for local
inspection but are not evidence fields.

Each authoritative platform script still creates a private same-directory
candidate and atomically replaces only the orchestrator's staging path. The
complete recipe does not touch a previously retained evidence set until all
five heavy commands and same-head validation pass. Its bounded publication
transaction keeps backups and restores them on failure or handled interruption,
so a partial platform result or mixed-head set is never accepted. Cleanup
failures, stale evidence, source changes, and Portal-owned worktree/runtime/
Compose leaks all fail the recipe. Headless and mobile records authenticating
the prior helper remain stale until the complete recipe is rerun and must not
be edited in place. Reproduction requires no global Docker
pruning, broad worktree/`target` deletion, virtual-device erase, or
`reverse --remove-all`.

Hosted CI does not execute this private-source boundary and does not upload
these files. Its required public/static job checks the harness order and cleanup
bounds, immutable pins, evidence validation and negative fixtures, no-secret
command construction, sanitized-only publication, and absence of a private
source credential or false hosted-execution claim.

This proves real mock-KYC Portal issuance on virtual mobile hosts only, against
an undeployed holder. Credential status remains `not_checked`. It does not
prove a camera, physical device, tailnet, real KYC, verified domain, production
discovery/trust, live holder DID, native-custody Portal restore, or resource
budget. Typed zeroization (#134), iOS/Android screen-privacy timing (#135), and
physical/tailnet work remain independent follow-ups.
