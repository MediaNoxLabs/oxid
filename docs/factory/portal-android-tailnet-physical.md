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

## Safety, evidence, and cleanup

Every retry uses a fresh offer, capability, app state, and runtime. Never reuse
a consumed offer. Require refusal with zero secret endpoint calls before explicit
consent, encrypted persistence, a real process restart, listing, and a fresh
reverification.

The command publishes only redacted mode-`0600` evidence after exact cleanup;
evidence must name the current Oxid `HEAD` and tree. It excludes device and
tailnet identities, endpoints, offers, capabilities, credentials, and protocol
secrets. Preserve ambiguous state for review. Remove only receipt-proven
Portal/process resources and a standalone stack this session owns or whose
owner expressly authorizes teardown.
