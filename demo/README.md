# Tailnet identity and Midnight demo

This kit starts or safely reuses Oxid's standalone Midnight node, indexer, and
proof server, exposes them to one physical Android phone through protected
Tailscale HTTPS/WSS routes, and installs the exact Oxid build. It is an
owner-invoked development demo, not CI, production, or release evidence.

Identity systems remain abstract protocol actors. No issuer, verifier, relying
party, or companion repository is fetched or managed by this kit.

## Protocol and transport boundaries

Tailscale is a private transport boundary, not an issuer/verifier trust anchor
or production discovery mechanism. The host launcher discovers its current
MagicDNS name and compiles these Midnight routes into the development app:

| Midnight service | Tailnet transport |
| --- | --- |
| Indexer GraphQL and WebSocket | HTTPS/WSS on port `8443` |
| Node WebSocket | WSS on port `10000` |
| Proof server | HTTPS on port `443` |

The OpenID4VC family is a separate protocol boundary. The current standalone
application truthfully exposes these provider-neutral roles:

| Protocol | Abstract service role | Current standalone capability |
| --- | --- | --- |
| OpenID4VCI 1.0 Final | Credential issuer | Embedded by-value, pre-authorized offer with an in-process issuer |
| OpenID4VP 1.0 Final | Credential verifier | Request preview, DCQL matching, explicit selection and consent; proof completion depends on the selected proving profile |
| SIOPv2 draft 13 | Relying party/verifier | Request-by-reference DID authentication with an in-process verifier |

The deterministic issuer, verifier, and relying party do not bind network
sockets and therefore do not need Tailscale. Arbitrary live identity-service
origins, generic runtime discovery, and unknown `openid4vp` request pairs remain
unavailable. Oxid rejects them rather than guessing whether a request is SIOP
authentication or OpenID4VP presentation. An external service may replace an
in-process adapter only after its HTTPS origin, request trust, response
delivery, and discovery policy are reviewed and composed explicitly.

## Preconditions

- Use a clean checkout of the exact Oxid commit you intend to demonstrate and
  enter `nix develop`.
- Start Docker Desktop and wait for `docker info` to succeed.
- Connect the laptop and phone to the same Tailnet. MagicDNS and HTTPS
  certificates must be available.
- Enable Android USB debugging, connect and unlock exactly one authorized
  physical Android phone, and accept its host authorization prompt.
- Stop every Android emulator and iOS Simulator. The canonical launcher rejects
  mixed or QEMU device selection.
- Ensure the Oxid host has no unrelated Tailscale Serve configuration. The
  standalone launcher will not overwrite another owner.

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
exists, it starts the Midnight node, indexer, and proof server, installs the
three protected Tailscale routes, and records that this demo owns them. If the
project already exists, the command reuses it only after read-only container,
finality-window, route, and HTTPS checks; an incomplete or unhealthy project is
never repaired or replaced.

After readiness succeeds, the command builds, installs, and opens the
compile-time `standalone-tailnet` Android application on the one selected
physical device. A cold first run can take a while because Nix, Docker, Rust,
and Android artifacts must be built. A successful command ends with:

```text
Oxid Tailnet identity demo: READY
```

Check the same exact-head receipt and live Midnight services at any time:

```bash
demo/status.sh
```

## Exercise the standalone capabilities

1. In Oxid, create or select the unique **Oxid Demo Wallet** development
   profile and accept the visible public-genesis warning.
2. Initialize its development protection and activate the Midnight account.
3. Refresh deployment readiness and balances. The app should report the
   `undeployed` Tailnet profile and independently ready indexer, node, and prover
   services before showing synchronized NIGHT, shielded, and DUST state.
4. Under **Documents** → **Manage identities**, create an active standalone DID.
5. For OpenID4VCI, load the standalone credential offer, preview the abstract
   issuer and credential, confirm consent, and choose **Accept and issue
   credential**. Successful verification and encrypted import complete this
   flow; the direct credential inbox is only a lower-level diagnostic.
6. For SIOPv2, load the standalone login request, preview the abstract relying
   party and purpose, confirm consent, and choose **Authenticate with DID**.
7. For OpenID4VP, load the standalone presentation request, review the abstract
   verifier, requested claims, and selected credential, then explicitly
   consent. Treat a truthful `proof_unavailable` result as the boundary of a
   build without the reviewed proving profile, not as a successful
   presentation.

Each request is single-use. After a refusal, failure, or completed operation,
load a fresh request rather than reusing protocol state. Do not copy credential
offers, request objects, tokens, nonces, proofs, credentials, device IDs, or
Tailnet identities into logs or issue comments.

## Stop

```bash
demo/stop.sh
```

Stop validates the exact Oxid receipt. If this demo created the standalone
project, it removes its three Docker containers and its owned Tailscale Serve
configuration. If another session owned the healthy standalone stack, the
stack and routes remain running. The installed app and its local wallet data
remain on the phone; removing either is a separate, explicit device action.

Missing, stale, permissive, symlinked, or ambiguous receipts fail closed and
preserve state for owner review. Never use global Docker deletion, `tailscale
serve reset`, or recursive worktree cleanup as a recovery shortcut.

## Boundaries

The scripts do not print or track device IDs, MagicDNS identities, offers,
request objects, capabilities, credentials, proofs, or protocol secrets.
Private runtime state is under `target/` with restrictive permissions. The kit
supports physical Android only; physical iOS signing and deployment are out of
scope.

For the underlying compile-time Tailnet profile and service ownership model,
see [ADR-0097](../docs/adr/0097-build-standalone-phone-routes-at-compile-time.md).
