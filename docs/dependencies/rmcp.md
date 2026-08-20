# Dependency review: rmcp (proposed, not yet adopted)

- Crate: `rmcp` — official Model Context Protocol Rust SDK
  (github.com/modelcontextprotocol/rust-sdk)
- Reviewed version: 3.1.3 (2026-08-17)
- License: Apache-2.0 (repository carries transitional MIT residue for
  older contributions; crate metadata is Apache-2.0)
- Status: **proposed** for the production `oxid-mcp` implementation
  (ADR-0099). The prototype deliberately uses no external dependencies;
  this review exists so adoption is a decision, not a scramble.

## Maintenance and adoption

Official SDK under the modelcontextprotocol organization; ~4.3M downloads
per month, used by ~2,300 crates; 60 releases with an active cadence.
Implements MCP 2026-07-28 (stateless revision) while remaining compatible
with 2025-11-25 and earlier; protocol negotiation is automatic.

## Why it fits Oxid's use case

- Dynamic tool registration (`ToolRouter::add_route`,
  `ToolRoute::new_dyn`) matches manifest-derived tools without
  compile-time macros; `disable_route`/`enable_route` map cleanly onto
  capability `status` changes, with `tools/listChanged` notifications.
- First-class `ToolAnnotations` (readOnlyHint/destructiveHint) match the
  tier policy.
- Elicitation support provides the future Tier-2 human-handoff primitive.

## Risks and mitigations

- Dependency tree includes `schemars` and macro crates — a material tree
  for a security-adjacent binary. Mitigation: exact-pin, `cargo deny`
  bans/licenses/sources gates, and keeping `oxid-mcp` out of default mobile
  builds.
- docs.rs coverage is thin (~48%); expect source-reading. Fallback crate if
  the API fights: `rust-mcp-sdk` 1.0.1 (MIT), the only credible
  alternative; `mcp-sdk-rs` is explicitly not production-ready.
- The 2026-07-28 revision deprecates stream-based server-initiated
  requests on a 12-month window; stdio remains fully supported. Track the
  Multi Round-Trip elicitation pattern (`request-state` feature) when
  implementing Tier-2 handoff.

## Exit strategy

The MCP surface is a thin protocol layer over the stable NDJSON protocol;
replacing rmcp with the fallback SDK or the current hand-rolled transport
is a bounded change confined to `apps/oxid-mcp`.
