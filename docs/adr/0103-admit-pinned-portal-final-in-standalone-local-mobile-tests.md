# ADR-0103: Admit pinned Portal Final in standalone-local mobile tests

- Status: Accepted
- Date: 2026-08-21
- Source: [issue #124](https://github.com/MediaNoxLabs/oxid/issues/124)
- Portal integration source: squash commit `925ec8d04882eabd4ac7b784c70fc2f0c152faae`, tree `58b4597524f88a0ae2253439a44dab0dc60cbb6f`
- Historical Portal PR head: `9c82db23eabe8b6d758b2731f2225910ea627c14`
- Profile source: `76e8edf394a4cb37ca822037272d543c68f25f71`; provenance SHA-256 `cf86f4ddb06131d7570c835e8c6c62d524e8179fe6a53436b20d2d4e72b44d87`
- Amends: ADR-0039
- Extends: ADR-0097, ADR-0101, and ADR-0102
- Implementation state: explicit iOS Simulator and Android QEMU `standalone-local` Portal test profile, sequential repository harnesses, and secret-free evidence are implemented; production and ordinary-mobile transport, native custody, tailnet Portal, physical-device, real-camera, live-holder-DID, production-trust, WebAssembly, and unsupported-grant paths remain unavailable

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
Portal HTTP feature. `oxid-app` rejects it outside iOS/Android and rejects every
native-custody or tailnet combination. The feature is not a public runtime
setting.

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

`just portal-mobile-smoke` is a fixed sequential recipe: iOS completes and
tears down before Android starts. Each platform command:

1. authenticates a clean Portal checkout, exact integration commit/tree,
   historical PR head, profile source, and provenance digest;
2. creates a detached temporary checkout of the exact landed commit and starts
   its real Nix/Docker composition;
3. recreates only the issuer with fixed loopback public origin and an
   Oxid-owned holder test resolver populated only from the app's persisted
   public DID record;
4. derives the canonical secret-free deployment manifest and creates an
   approved mock-KYC session;
5. keeps the real offer in memory behind a loopback, no-store test control
   endpoint;
6. counts only HTTP path classes through a body-blind proxy, proving refusal
   makes zero token, nonce, or credential calls;
7. drives the existing native one-item ingress/router, Dioxus preview/consent,
   managed proof, strict exchange, verifier/sink, encrypted store, and
   restart/list/reverify path;
8. on successful completion, empties only its named Portal Compose project and
   removes the detached checkout, platform-private runtime, and owned lock.

The cleanup hardening delivered at Oxid `afaeee5` installs the EXIT cleanup
owner before the lock, state directory, fetch, worktree, support process, FIFO,
Compose stack, or manifest can be created. Success, startup/test failure,
interrupt, and termination therefore share one scoped cleanup path. It always
removes `target/portal-mobile-e2e/<platform>/runtime`; it also bounds support
shutdown, attempts exact named-project teardown, removes the detached worktree
and owned lock, and reports cleanup failure instead of silently succeeding.
Android removes only the dynamically allocated CDP forward. It neither removes
unrelated Docker/worktree/`target` state nor uses broad virtual-device or ADB
cleanup.

Neither `simctl openurl` nor `am start -d` receives the offer. Both deliver
only a fixed, non-secret trigger URL under the existing
`openid-credential-offer` scheme
(`openid-credential-offer://standalone-portal-test-fetch`). The app, built with
the compile-time `loopback-test-offer-trigger` feature that
`standalone-portal` selects, recognizes only that exact literal. A named
background worker fetches the real offer over a time- and size-bounded
loopback-only HTTP GET before handing a validated result to the existing
one-item router; Tao/Wry's OS callback never blocks on retrieval. No real
offer or grant enters host/device argv, OS URL/intent state, a staging file,
logs, or retained evidence. A failed fetch enqueues the inert trigger string,
which the strict `openid-credential-offer` route rejects as malformed rather
than ever treating it as a real grant. The QEMU gate cold-reboots an
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

Evidence under `target/portal-mobile-e2e/{ios,android}/evidence.json` is an
ignored, closed boolean/source-pin/platform schema. It may record virtual model,
OS/API, application id, and fixed reverse ports. It excludes simulator/emulator
identifiers, routes, DIDs, grants, tokens, nonces, JWTs, credential bytes,
proofs, private parts, claims, logs, PIDs, and timestamps; a sentinel scan
rejects common representations. The selected virtual device's installed Oxid
app/data and normal build outputs remain available for local inspection but are
not evidence fields. Android deliberately retains the exact app reverse entries
for 8088, 9944, 6300, 18090, 18091, and 18093; only the owned dynamic CDP
forward is removed. A rerun resets only Oxid app data and recreates its scoped
runtime. Evidence is generated into a private same-directory candidate, checked
against the exact schema and sentinel, and published with an atomic rename.
Generation, schema, sentinel, or publication-finalization failure preserves any
prior valid evidence. Publication precedes the EXIT cleanup transaction, so a
later cleanup failure still fails the command but may leave the newly validated,
clean-head-bound evidence in place. The retained evidence's `oxid.head` binds it
to its source commit. Reproduction therefore requires no global Docker pruning,
broad worktree/`target` deletion, virtual-device erase, or `reverse --remove-all`.

This proves real mock-KYC Portal issuance on virtual mobile hosts only, against
an undeployed holder. Credential status remains `not_checked`. It does not
prove a camera, physical device, tailnet, real KYC, verified domain, production
discovery/trust, live holder DID, native-custody Portal restore, or resource
budget. Typed zeroization (#134), iOS/Android screen-privacy timing (#135), and
physical/tailnet work remain independent follow-ups.
