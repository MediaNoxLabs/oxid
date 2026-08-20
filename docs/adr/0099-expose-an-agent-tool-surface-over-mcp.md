# ADR-0099: Expose an agent tool surface over MCP

- Status: Proposed
- Date: 2026-08-19
- Blueprint source: Sections 1, 3–7, 11–13, 16, 18, and 21
- Prototype source: `apps/oxid-mcp` in this repository (dependency-free bridge; live-verified against the standalone headless wallet)
- Tracking: issue #70 (roadmap) and issue #69 (manifest confirmationRequired gap, resolved)
- Implementation state: proposed; a dependency-free production bridge plus a
  test-only manifest dependency demonstrate manifest-derived tool generation,
  the fail-closed tier filter, and combined-manifest conformance; consent,
  authority, cancellation, and reconciliation methods are excluded

## Context

AI agents are becoming first-class consumers of wallets. The 2025–26
ecosystem (Coinbase Agentic Wallet, Phantom and MetaMask agent surfaces,
self-hosted policy engines) has converged on a pattern: agents act only
inside a pre-authorized, infrastructure-enforced envelope; humans keep an
out-of-band approval channel the agent cannot see; keys are never reachable
from the tool surface. On the identity side, EUDI "sole control" makes the
rule sharper: an agent must never be the party that consents to credential
presentation — deployed precedents (Vidos MCP + OpenID4VP) give agents only
initiate/check tools while consent happens in the human's own wallet.

Oxid is unusually well positioned: `oxid-headless` already exposes a
versioned NDJSON protocol whose capability manifest truthfully labels every
method (`status`, `confirmationRequired`, `secretsExposed`,
`claimValuesExposed`, and related exposure flags). A Model Context Protocol
server can therefore derive its tool surface from the wallet's own
self-description instead of maintaining a second, drifting list.

## Decision

Add an MCP stdio server that bridges to `oxid-headless` as a child process
and derives its tools from `system.capabilities` at startup, filtered by a
fail-closed, three-tier policy:

- **Tier 0/1 — exposed to agents:** `ready`, non-alias methods with no
  `confirmationRequired` flag, no exposure flags
  (`secretsExposed`, `claimValuesExposed`, `privateMaterialExposed`,
  `rawCredentialExposed`, `serializedTransactionExposed`,
  `requestUriExposed`), and — as defense in depth after issue #69 — no
  authority verb (`accept`, `authorize`, `sign`, `submit`, `send`,
  `delete`, `deactivate`, `forget`, `refuse`, `recover`, `restore`, `cancel`,
  `reconcile`, `import`, `quit`) anywhere in the method name. Read/status/preview
  methods carry `readOnlyHint: true` annotations.
- **Tier 2 — never exposed:** every consent, authorization, signing,
  submission, deletion, and recovery ceremony. These stay in the human's
  wallet surfaces. The server's `instructions` string states this policy to
  the client.
- Process-lifecycle methods (`system.quit`) are denylisted outright.

The bridge holds no keys, no credentials, and no policy state: it is a
protocol translator whose most dangerous capability is calling
already-agent-safe wallet methods. stdout carries MCP only; diagnostics go
to stderr.

## Consequences

- AI agents (Claude, and any MCP client) gain a safe window into Midnight
  assets, sync state, transaction history, DID inventory, and credential
  metadata — the "all-in-one identity tool" surface — without any path to
  moving value or disclosing claims.
- The manifest becomes a load-bearing security boundary for a second
  consumer, which issue #69 shows it is not yet fully ready for: the
  `confirmationRequired` gaps must be fixed and conformance-tested.
- Future work (issue #70) is explicitly out of scope here: a policy engine
  below the tool layer (budgets, allowlists), out-of-band human approval
  handoff for Tier-2 escalation, delegation credentials (AP2-style mandates
  as verifiable credentials — a natural fit for an identity wallet), and
  adopting the official `rmcp` SDK (reviewed in `docs/dependencies/rmcp.md`)
  once the surface stabilizes.
- The prototype deliberately adds zero external dependencies; its only new
  test dependency is the workspace capability-manifest crate used to prove
  the composed policy. The hand-
  rolled JSON-RPC surface is ~100 lines and targets MCP 2025-03-26 stdio,
  which current clients accept. Production hardening (protocol revisions,
  elicitation, listChanged) is where `rmcp` earns its adoption.
