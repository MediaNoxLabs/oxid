# ADR-0105: Recover owner Midnight roots for PreProd observation

- Status: Accepted
- Date: 2026-09-02
- Source: Blueprint §§3–8, 12–13, 16–18, 21; ADR-0071/0074/0090/0098; issue #244
- Prototype source: `midnight-ledger` `mobile-prototype@255f2caf` for balance behavior only; embedded prototype seeds and key files are explicitly rejected
- Implementation state: opt-in Android/iOS composition, empty-profile root recovery, native one-shot custody, canonical account derivation, observation-only UI, and guarded launcher implemented; owner-entered live PreProd evidence remains manual
- Amends: ADR-0071 and ADR-0098

## Context

Oxid already derives and synchronizes authoritative public NIGHT, protected
shielded-token, and DUST state, and its mobile adapter stores one device-bound
sealed vault per wallet profile. The reviewed prototype demonstrates PreProd
balances from a 32-byte root, but selects that root from a build-time file or
environment variable and exposes it to ordinary application state. Copying
that mechanism would bypass Oxid custody and make release artifacts, logs, or
automation potential secret-distribution channels.

ADR-0098 authenticates an atomic deployment envelope and checks the live node
genesis before composition, but deliberately selects no deployment in the
default application. A deliberate owner recovery journey therefore needs a
separate incoming port, an explicit deployment profile, and a narrower UI than
the existing transaction-capable standalone wallet.

## Decision

Accept exactly one owner root representation: 32 bytes encoded as 64 lowercase
ASCII hexadecimal characters. Whitespace, prefixes, uppercase, mnemonic words,
and every other width or encoding are rejected. The typed root and UI buffer
redact `Debug`, use zeroizing storage, and never enter URLs, environment
variables, profile metadata, WebView storage, diagnostics, analytics, crash
copy, or repository files.

Expose root recovery only in the separately selected `preprod-observation`
mobile build. The build embeds public signed deployment material and its public
verification key, verifies audience, sequence, validity, signature, HTTPS/WSS
routes, and the exact live node genesis, then fixes the application service to
the authenticated `preprod` network. Runtime environment variables cannot
replace its endpoints, trust root, network, or custody adapter. The existing
inert `.invalid` SSI routes remain unavailable and make no SSI deployment
claim.

Recovery requires an empty public profile, explicit human confirmation, and a
fresh platform authorization at the native sealed-vault initialization
boundary. Initialized custody, an existing account association, an alternate
network association, missing native protection, and authorization denial all
fail closed. Native initialization atomically creates the vault or leaves it
uninitialized; it never merges or overwrites. A denied authorization may leave
only the public authenticated PreProd selection staged. Exactly that
no-account state is admitted for a retry; every other partial state is
rejected.

After installation, derive canonical account index 0 and address index 0
through existing protected derivation ports. No root or child secret is
returned. The Wallet page reuses its single `Sync now` action and independently
projects public NIGHT, shielded tokens/notes, and DUST with their existing
live/cached/stalled freshness. Network selection is fixed, and the
observation-only UI omits DUST registration, transfer preparation,
authorization, proving, submission, and submission recovery controls.

The secret input is cleared before work is scheduled and again on cancel,
navigation/unmount, failure, and mobile lifecycle wake. Intermediates remain in
zeroizing containers where Rust permits. Native implementations retain their
platform guarantees and session limits; no background render or profile read
may prompt for custody.

Successful recovery does not create a second seed record. The existing
complete encrypted backup flow may subsequently export the protected vault and
public associations after fresh authorization. Existing fresh-install backup
recovery remains separate and cannot merge into this profile. Chain-derived
caches remain rebuildable and are not treated as backup authority.

## Consequences

- A normal Oxid build contains neither the PreProd deployment/profile feature
  nor the root-recovery UI copy; explicit feature and target guards prevent
  accidental selection.
- The owner must retain the original seed or create and protect a complete
  Oxid backup after recovery. Oxid does not display the seed again.
- Startup requires current trusted time plus successful signed-profile and
  node-genesis authentication. An offline, stale, mismatched, or plaintext
  deployment fails before the recovery capability exists.
- Live PreProd balance evidence is owner-run because CI receives no funded
  credential. `just android-preprod-observe` builds and installs the explicit
  profile without accepting a seed or endpoint argument; the owner enters the
  root only on the device.
- Transaction-capable PreProd product composition, mnemonic import, alternate
  account discovery, SSI endpoints, and cloud custody require later decisions.

## Verification

- Application tests cover canonical/malformed input, redacted commands,
  confirmation ordering, initialized or associated profiles, authorization
  denial/retry, canonical derivation, and duplicate rejection.
- Mobile-storage tests cover denial without initialization, one-shot install,
  deterministic derivation after adapter restart, and duplicate refusal.
- UI tests cover redacted/cleared secret state and the observation-only write
  control policy.
- Deployment tests authenticate the exact signed current profile and reject
  not-yet-valid and stale time without network access; live startup separately
  checks node genesis.
- Release guards prove ordinary feature graphs and the normal release artifact
  exclude the PreProd profile, endpoint/profile markers, and recovery UI copy.
