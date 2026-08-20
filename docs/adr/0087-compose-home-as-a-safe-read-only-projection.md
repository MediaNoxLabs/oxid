# ADR-0087: Compose Home as a safe read-only projection

- Status: Accepted
- Date: 2026-08-19
- Blueprint source: Sections 1, 3–7, 12–13, 16, and 18
- Design source: `docs/design/information-architecture.md` Home anatomy, `docs/design/design-system.md`, and `docs/design/rollout.md` Phase 1b
- Prototype source: `midnight-ledger` commit `074b1a4bccbfee1740ee188374b606a022ecef42`, `mobile-bench/dioxus-wallet/src/app.rs`
- Tracking: issues #2, #65, and #79
- Implementation state: Home projects existing safe account, security, shielded-sync, credential, and Passport Vault views while Wallet retains every operational control

## Context

ADR-0086 deliberately rendered the complete operational account page on both
Home and Wallet. That one-slice overlap made the route-shell migration
non-destructive, but it is not the accepted Home experience. Home must answer
the glanceable product questions—balance, source, products, security posture,
and recent activity—without becoming another application service or a second
owner of wallet transitions.

The prototype's Assets tab combined address, balance, network selection, sync,
and development controls in one Dioxus component. Oxid already migrated those
behaviors behind typed use cases and retains them on Wallet. Copying the
prototype component or introducing a Home aggregate in the application layer
would duplicate authority and couple a presentation redesign to core contracts.

Some requested trust signals are not currently observable. The security view
reports protection state and class, user-presence requirements, and whether a
portable backup is supported; it does not report that a backup has been
completed or that biometrics are enrolled. Home therefore cannot truthfully
render “Backed up” or “Biometrics” as completed facts yet.

## Decision

Implement Home entirely inside the Dioxus incoming adapter as a read-only
projection over existing incoming use cases. One background UI worker reads:

1. the selected account and security status through the same non-interactive
   account loader used by Wallet;
2. the profile-scoped shielded-sync status;
3. the profile-scoped public credential inventory; and
4. the public Passport Vault view.

Each optional read can fail independently. Home renders a bounded unavailable
state for that product instead of failing the whole screen or displaying a raw
adapter error. The account/security read is the required root; if it fails,
Home renders one payload-free error with Retry. No Home read initializes,
unlocks, derives, syncs, authorizes, proves, submits, reconciles, imports, or
mutates any state.

Home contains the accepted five-part anatomy:

- a NIGHT hero with DUST and reviewed source/synchronization labels;
- Receive, Send, Present, and Scan actions;
- full-width horizontally scrollable cards for NIGHT, shielded assets, the
  newest credential, and Passport Vault;
- a security strip naming only current protection state/class and backup
  capability; and
- at most three rows from the existing account transaction projection.

Receive and Send select Wallet, where their existing complete controls remain.
Present selects Documents. Scan calls the same shared Dioxus scan starter used
by the center action, which uses the existing `QrScannerPort` and
`RouteIdentityRequestUseCase` before it publishes a pending request. Product
cards route to Wallet, Documents, or the existing Passport Vault secondary
route. “See all” selects Activity. Security opens Settings.

The credential card may render the public display name, reviewed format label,
and verification label. It must not render claims, subject/issuer DIDs, opaque
credential identifiers, or issuance timestamps. The activity preview may
render reviewed direction, status, and known NIGHT/DUST amounts; it must not
render transaction identifiers, counterparties, cursors, block heights, or
epoch values. Identity and Vault events remain absent until an application
interaction-log contract exists.

## Security and architecture boundaries

- Home consumes only existing Oxid-owned public application views. It has no
  storage, Midnight, SSI, protocol, or platform adapter dependency.
- The projection is display state, not spend authority. Cached, simulated, and
  unavailable account sources retain their reviewed labels and cannot enable a
  transaction.
- Optional product failures become closed UI states and never interpolate
  adapter payloads.
- Backup support is labelled “available”, never “completed”. Device, hardware,
  development-only, and unavailable protection classes stay distinct. No
  biometric claim is inferred from a user-presence requirement.
- Scan classification, the single-pending-request rule, preview, and explicit
  consent are unchanged.
- Secret mode remains Phase 4 work. Home does not add an eye control or imply
  shoulder-surfing or OS-snapshot protection.

## Consequences

- Home and Wallet no longer duplicate the complete account page, while every
  migrated wallet and SSI behavior stays reachable.
- Home can degrade one product card at a time and remains useful when
  credentials, shielded state, or Vault state is unavailable.
- The presentation adapter performs several read-only queries. If a future
  measured performance budget requires one snapshot, that requires an explicit
  application read-model decision rather than silently moving UI concerns into
  the core.
- The security strip cannot display backup completion until a durable,
  independently testable application fact exists.
- The recent feed is intentionally transaction-only. A unified identity/Vault
  interaction log requires a later application event/read-model decision.

## Validation

- Unit tests cover Home quick-action routing, newest-credential selection,
  payload-free transaction summaries, and truthful security labels.
- The Dioxus copy and CSS/token gates cover every new label and class.
- iOS XCUITest and Android CDP smoke flows assert the five Home regions and
  continue through the full standalone Wallet, DID, credential, and Vault
  journeys.
- `just ios-smoke` and `just android-smoke` remain the Tier-1 mobile gates.

## Rejected alternatives

- Keeping the full Wallet page on Home would leave Phase 1b incomplete and
  preserve two competing operational surfaces.
- Adding a cross-hexagon Home aggregate now would change application contracts
  for a presentation-only slice without a measured need.
- Claiming “Backed up” from backup support or “Biometrics” from required user
  presence would overstate facts the application does not expose.
- Copying identifiers, block heights, DIDs, or claims into the preview would
  violate progressive disclosure and expand the shoulder-surfing surface.
- Implementing Receive or Send directly on Home would pre-empt the reviewed
  Phase 2 ceremonies and risk duplicating state-machine transitions.
