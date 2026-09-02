# Lace ID Portal physical Android demo

This kit starts or safely reuses the local Midnight standalone services, starts
one receipt-supervised Lace ID Portal session, installs the pinned Oxid build on
one physical Android phone, and opens a temporary Tailnet-only HTTPS Portal page
on the laptop. It is an owner-invoked development demo, not CI or release
evidence.

## Preconditions

- Use a clean checkout of the exact Oxid commit you intend to demonstrate and
  enter `nix develop`.
- Start Docker Desktop and wait for `docker info` to succeed.
- Connect both the laptop and phone to the same Tailscale network. MagicDNS and
  HTTPS certificates must be available to the Tailnet.
- Enable Android USB debugging, connect and unlock exactly one authorized
  physical Android phone, and accept its host authorization prompt.
- Stop every Android emulator and iOS Simulator. The canonical launcher rejects
  mixed or QEMU device selection.

Verify the device boundary without copying its identifier into notes or logs:

```bash
adb devices
```

There must be exactly one non-emulator row in the `device` state.

## Start

```bash
demo/start.sh
```

The command first queries the fixed `oxid-standalone` Docker project. If none
exists, it starts the node, indexer, proof server, and three protected Tailscale
routes and records that this demo owns them. If a project already exists, it is
reused only after a read-only container, finality-window, route, and HTTPS
check; an incomplete or unhealthy project is never repaired or replaced.

The command then delegates to the canonical manual Portal lifecycle. That
lifecycle fetches the pinned Lace ID Portal `integration` revision into private
runtime state, builds its consumer images, creates a fresh issuer and one-shot
offer, adds one temporary HTTPS listener, builds and installs Oxid, and opens
the Portal page. A cold first run can take a while because Nix, Docker, Rust,
and Android artifacts must be built. A successful command ends with:

```text
Oxid Portal Tailnet demo: READY
```

Check the same exact-head receipts and live processes at any time:

```bash
demo/status.sh
```

## Issue and receive a credential

1. In Oxid, open **Wallet** and activate the development wallet if needed.
2. Open **Documents** → **Manage identities** and create a standalone DID.
3. Tap **Publish active holder DID to test issuer** and wait for confirmation.
   This sends only the public DID Resolution Result to this temporary test
   issuer; private key material stays in the phone's protected wallet.
4. In the Portal page opened on the laptop, complete the mock identity flow and
   approve it. The page moves to the credential-offer QR.
5. In Oxid, select **Scan QR** and scan that QR exactly once.
6. Review the credential offer and choose **Accept and issue credential**.
7. Expect the preview to close and **Saved to your wallet** to appear. Issuance,
   verification, and encrypted storage have completed; do not invoke a separate
   standalone receive action.
8. Open the Digital Passport card and choose **Reveal locally** to inspect the
   protected attributes. Hide them again before sharing the screen.
9. Restart Oxid and confirm the credential remains listed. Reverify it from the
   credential card to exercise fresh issuer/DID resolution.

If a scan or issuance fails, do not reuse the QR. Stop the session and start a
new one so the offer, capabilities, holder publication, and runtime are fresh.

## Stop

```bash
demo/stop.sh
```

Stop validates the exact Oxid/Portal/session receipts, asks the canonical manual
lifecycle to remove only its owned resources, and restores the Tailscale Serve
configuration that existed immediately before the Portal listener was added.
If this demo created the standalone project, it then stops that project and its
owned protected routes. If another session owned the healthy standalone stack,
the stack and routes remain running.

Missing, stale, permissive, symlinked, or ambiguous receipts fail closed and
preserve state for owner review. Never use global Docker deletion, `tailscale
serve reset`, or recursive worktree cleanup as a recovery shortcut.

## Boundaries

The scripts do not print or track device IDs, MagicDNS identities, offers,
capabilities, credentials, or protocol secrets. Private runtime state is under
`target/` with restrictive permissions. The demo supports physical Android
only; physical iOS signing and deployment are out of scope.

For the underlying evidence and safety model, see
[Portal physical Android Tailnet lane](../docs/factory/portal-android-tailnet-physical.md).
