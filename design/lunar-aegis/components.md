# Component contracts

The UX Pilot system has 20 React-oriented examples. This document captures
the reusable behavior and visual state expected in Oxid's Dioxus UI. It is
the local handoff while UX Pilot's MCP returns collection metadata without
component bodies. The exported [screen HTML](screens/html/) shows composition
examples; it is not the component implementation.

| UX Pilot component | Oxid use | Required states or constraint |
| --- | --- | --- |
| `LunarAegisLogo` | Welcome and compact header | Use the supplied mark; resolve legibility and concept/SVG mismatch at icon sizes. |
| `TabBar` | Primary five-slot navigation | Home, Wallet, Scan, Documents, Activity; local SVGs, full labels, one active state, safe-area padding. |
| `Navbar` | Screen heading and back/settings actions | Preserve task title and predictable back destination. |
| `Button` | Primary, secondary, and blocked actions | Minimum 44 × 44 px; inactive state when prerequisite or authorization is absent. |
| `Input` | Recipient, search, and form fields | Neutral placeholders, errors, and large-text support; never prefill an invented address. |
| `Card` | Asset, identity, and settings surfaces | 24 px target radius; source/freshness or empty state remains legible. |
| `Badge` | Capability and status labels | “Verified” only with evidence; pending, simulated, unavailable, and unknown are distinct. |
| `Alert` | Warnings and recovery guidance | State the actual risk or next step without promising a proof. |
| `Spinner` | Bounded loading | Pair with task-specific text and eventual failure/retry path. |
| `Tabs` | Local mode switching | Do not silently change network or custody context. |
| `Dialog` | Focused confirmation or error | One explicit decision, clear cancel path, accessible focus. |
| `Text` | Type hierarchy | Space Grotesk / Plus Jakarta Sans / JetBrains Mono roles with Noto fallback. |
| `Stack` | Layout rhythm | Use the 4–48 px spacing scale and mobile safe areas. |
| `Checkbox` | Optional attribute selection | Start unselected; never imply a credential or proof exists. |
| `ConsentReview` | Presentation/login disclosure | Show exact requester, purpose, claims, source, and refusal; approve only after binding to the request. |
| `AddressQR` | Receive address | With no controlled address, show “Address unavailable”; no usable QR, Copy, or Share. |
| `ActivityRow` | Wallet event history | Distinguish local/simulated/cached/network-sourced events and pending/unknown outcomes. |
| `CredentialCard` | Document inventory/detail | Show issuer and verification evidence; empty and unverified states first. |
| `EmptyState` | No account, document, or activity | One actionable prerequisite, no fake populated content. |
| `OutcomeCard` | Send/receive/recovery result | Pending and unknown are not success; retain safe recovery guidance. |

## Composition rules

- A screen uses the shared navigation icons and token roles. Active state
  should use color **and** a non-color cue.
- Every list-bearing surface needs loading, empty, error, and populated
  renderings. Proposed populated examples stay labeled illustrative.
- Consent and transfer review show the exact object that will be authorized.
  Do not hide the amount, recipient, or claims on authorization surfaces.
- QR capture classifies first and routes to explicit review. Scanning alone
  does not send, accept, or disclose.
- The UX Pilot gallery still has stale *default preview fixtures* for
  AddressQR, Button, Alert, and Checkbox. These explicit demo props are not
  approved product data. See [qa.md](qa.md).
