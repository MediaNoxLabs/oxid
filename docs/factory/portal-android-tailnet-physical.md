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
never a replacement for the automated physical or simulator lanes. From the
same clean, healthy preconditions, start one fresh session:

For the complete checkout-to-cleanup operator path, use the root
[demo kit](../../demo/README.md). The lower-level commands below remain the
canonical Portal-session lifecycle used by that kit.

```bash
just portal-tailnet-manual-start
just portal-tailnet-manual-status
```

Start creates and validates the same private mode-`0600` pinned mock transform
used by the browser contract, then exposes its KYC page under the receipt-owned
same-origin HTTPS `/kyc` mount. It opens the Portal page in the Mac browser and
prints the one permitted public page URL; status intentionally reveals no
payload. This owner demo remains non-evidence.

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
already performed issuance, verification, and encrypted persistence; do not
use a second receive action. Normal and demo profiles do not expose the
standalone credential inbox because it bypasses OpenID4VCI and is reserved for
the explicit developer profile. A Digital Passport card lists the validated
first name, last name, date-of-birth predicate, optional document number, and
issuing state capabilities. Selective attributes remain encrypted until the
holder taps **Reveal locally**; the date of birth remains predicate-only.

Do not retry or reuse a consumed QR. Stop before a fresh attempt:

```bash
just portal-tailnet-manual-stop
```

Stop validates the session/process/Serve receipts, removes only owned Portal
state, and restores the exact prior Serve baseline. If a receipt is ambiguous,
it fails closed for owner review rather than deleting shared state.

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
