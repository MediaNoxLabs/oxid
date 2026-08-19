# Core Journeys, Redesigned

Each journey lists: the current implementation (so the delta is buildable),
the redesigned flow with a step budget, and the invariants it must keep.
State machines named here are the existing ones in
`crates/ui-dioxus/src/lib.rs` — they are the contract; only presentation
changes.

## 1. Onboarding (fresh install → usable home)

**Today (ADR-0090 / issue #82):** fresh install opens with exactly **Create new
wallet** or **Restore from backup**. Create names/selects the profile without
showing its opaque identifier, then offers skippable device protection before
Home. Restore owns a separate component lifetime, so Back drops its local
zeroizing secret state.

**Redesign (budget: ≤ 60 seconds, 3 screens):**
1. Welcome: product name, one sentence, two buttons — **Create new wallet**
   / **Restore from backup** (the universal fork; restore leads to the
   existing recovery flow re-housed as its own path, not a stacked card).
2. Name sheet: single field, smart default, one button. No profile id shown
   — ever. (The id remains available in dev profile / details.)
3. Biometrics offer: one screen, one toggle, skippable (progressive
   security). Then land on Home with a soft celebration and the security
   status strip showing what's left ("Back up your wallet — 2 min").
   No seed phrase exists in this flow: Oxid's backup model is the encrypted
   complete-wallet export, which is *deferred* — prompted contextually after
   first value/credential arrives, monobank/Phantom style.

**Invariants:** profile creation still routes through the same use case;
recovery still requires the empty-destination + explicit confirmation
semantics (rephrased to one sentence + one checkbox).

## 2. Receive (budget: 1 tap)

**Today (ADR-0091 / issue #83):** Home opens a non-primary Receive sheet in one
tap. It reads the existing account projection off the UI executor, refuses to
present fixture/watch-only addresses as holder-controlled before protected
derivation, and renders only returned rails as human-labelled segmented
capsules (Public / Private / Fee account / neutral fallback). The selected full
address alone feeds the large deterministic QR and typed native Copy/Share
ports; the grouped middle-truncated preview is display-only. Close pops the
sheet back to Home, while an unactivated profile gets one truthful path to the
Wallet activation surface.

**Remaining redesign:** payment requests, receive amounts, address rotation or
discovery, and deeper rail details require separate reviewed domain/port work;
none are represented as inert controls.

## 3. Send (budget: 4 steps + confirm sheet + biometric)

**Today (ADR-0088 / issue #80):** two editable screens collect recipient and
amount/privacy, then the existing nine-state `TransferPanelState` supplies an
exact preview-derived summary, collapsed details, a separate authorization
sheet, explicit prove/submit intent, and truthful Sending / Confirmed / Failed
recovery states. Safe cancellation, retained drafts, and network reconciliation
remain application-owned.

**Redesign — the canonical wizard, one decision per screen:**
1. **Recipient**: bounded manual entry and the development self-address
   affordance are delivered. Clipboard import, payment-address scanning, and
   recent recipients require reviewed ports and are intentionally not rendered
   yet. Exact adapter validation remains authoritative.
2. **Amount**: big numerals, NIGHT with decimal formatting (the exact-integer
   formatter already exists — no more "base units"), privacy toggle
   (Public/Shielded) as a visible choice with a one-line explanation of the
   difference, balance and "max" affordance. Fee shown as "calculated when
   proving — never more than your balance allows" until known.
3. **Review**: human summary sentence ("Send 12.5 NIGHT privately to
   mn1…k29x") above the detail rows (collapsed behind "Details").
4. **Confirm sheet + biometric**: the authorization moment. One sentence,
   amount + recipient repeated, biometric/OS prompt, then live status.
   Status then follows the 3-state convention users know — **Sending
   (amber, with cancel-before-broadcast while the machine allows it) →
   Confirmed (green, celebratory tick) → Failed (red, with the three typed
   recoveries as human choices)**: "Edit and try again" / "Retry safely —
   nothing was broadcast" / "Check with the network" (reconcile). The
   never-blind-retry copy stays, rephrased: "This may have reached the
   network. Oxid will check before anything is sent again."

**Invariants:** the 9-state machine, retained-draft recovery, 50 ms
cancellation polling, persist-before-broadcast semantics, `Submitting`
ambiguity handling. The wizard is a *view* over the same states.

## 4. Sync (DUST / shielded) — from foreground chore to background state

**Today:** two symmetric panes with cursor numbers and Sync/Resync/Cancel
buttons, gated on unlock (lib.rs:2798-3231).

**Delivered core (ADR-0090 / issue #82):** sync is one account card with a
single **Sync now** action over public account refresh plus the independently
authoritative DUST and shielded sessions. While either session runs, the action
becomes **Cancel sync**. Combined progress and human states replace cursor and
event-count prose; cached/cancelled/stalled remain non-authoritative. Native
background scheduling, pull-to-refresh, relative-freshness copy, and the Send
amount-step treatment remain follow-up work because no reviewed platform event
or freshness projection exists yet.

## 5. Add a document (credential issuance)

**Today (ADR-0089 / issue #81):** paste or classified Scan ingress leads to an
ordered WHO → WHAT → FROM → WHY offer review. It names the issuer endpoint and
truthfully warns when no production trust signal exists, names every offered
document, explains protected managed-DID binding, and states the holder-local
storage outcome before the existing literal consent boundary.

**Redesign (EUDI-conventional):** entry via Scan or "Add document" →
**issuer identity block** (name + verified-domain line, visually distinct
trust treatment) → **credential preview card** (issuer-branded when display
metadata exists, designed fallback otherwise) → one consent sentence
("Woodgrove City will issue you a Digital Passport — you choose where it's
used") → Accept (biometric) / Not now. Spec names (OpenID4VCI etc.) move to
the detail sheet. The standalone fixture button survives in the demo
profile's drawer (ui-profiles.md).

## 6. Present / prove (the flagship consent sheet)

**Today (ADR-0089 / issue #81):** paste or classified Scan ingress leads to the
four-question sheet below. One match is shown and selected; multiple matches
require the user to choose a card before the consent checkbox is enabled
(ADR-0082). Every claim in the current prepared plan is required and locked on;
the age predicate says both what it confirms and that date of birth is not
shared. Standalone verifier endpoints are explicitly unverified.

**Delivered core — one sheet, four questions, in order:**
1. **WHO**: verified relying-party name + domain, with a registered/verified
   indicator; explicit warning state when unverified.
2. **WHAT**: per-attribute checklist — required items locked-on, optional
   items **off by default**; predicates rendered as human sentences with
   negative reassurance: "Confirms you're over 18. Your date of birth will
   **not** be shared."
3. **FROM**: the credential card being used — chooser when multiple match.
4. **WHY**: the verifier's stated purpose, one line.
Then a single affirmative button ("Share proof") retains the existing protected
authorization boundary. Refusal is one tap, no guilt copy. First-proof
celebration and a typed identity Activity entry remain follow-up work because
the current application surface has no durable interaction-log contract.
When proving isn't available (no artifact root), the sheet says so in one
sentence — "This build can't generate proofs yet" — with the full truth in
Details; `proof_unavailable` never reaches the user raw.

## 7. Passport Vault (product card → section)

**Today:** one page stacking a 64-char hex contract-address field, duplicate
create-lock forms, and a second full lifecycle machine rendered as card
swaps (lib.rs:4888-5925) — currently half-unstyled due to missing CSS
classes (filed as a bug).

**Redesign:** the Vault card on Home opens a section with: vault balance +
lock list as cards; contract connection presented as a saved, named
connection (address entry once, then remembered and truncated — never
retyped); create-lock as one form (merge the duplicates); and the
call lifecycle as an explicit **stepper** (Review → Authorize → Prove &
submit) matching the Send wizard's ceremony. Simulation/authentication
labels become pills with one-tap explanations ("Simulated — this vault runs
locally; nothing settles on Midnight").

## 8. Backup & recovery (celebrated, not buried)

**Today (ADR-0090 / issue #82):** complete export remains in Settings and
fresh-install recovery lives behind onboarding's Restore fork. After encrypted
archive creation and the native document exporter both succeed, Oxid records a
profile-scoped timestamp-only receipt, celebrates **Backup complete**, and may
show **Backed up** on Home/Settings. Cancel, error, and restored archives never
fabricate that receipt; copy warns that the external document can later move or
disappear.

**Redesign:** one **Backup** surface in Settings + a contextual Home prompt
after first value: choose secret (with strength meter and the "store it
separately" rule as one sentence), confirm, native file dialog, then a
**celebration** and the security strip flips to "Backed up ✓". Recovery
lives in onboarding's Restore fork and in Settings; the empty-destination
requirement is stated as one sentence ("Recovery needs a fresh wallet — this
one already has data"). Invariants: v3 KDF parameters, zeroization,
constant-time comparison, one-checkbox explicit consent (rephrased).

## 9. Scan ingress (unchanged plumbing, elevated placement)

The center Scan action feeds the existing classifier
(CredentialIssuance / CredentialPresentation / SelfIssuedAuthentication →
plus payment addresses), pushes the right route with a preview banner, and
keeps the single-pending-request rule ("a new event never replaces active
holder review") — now expressed as a queued badge instead of a silent drop.
