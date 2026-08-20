---
name: agent-review
description: "Use for pi.dev agent peer review through the pinned agent-review-pi extension and its native review tools."
---

# Agent Review loader

1. Require the project-local package at
   `../../npm/node_modules/@input-output-hk/agent-review-pi/package.json` to be
   present with version `0.5.0`. If it is absent or has another version, stop
   and ask the operator to enter the repository through `nix develop` with a
   GitHub token that can read packages.
2. Read
   [`../../npm/node_modules/@input-output-hk/agent-review-pi/skills/agent-review/SKILL.md`](../../npm/node_modules/@input-output-hk/agent-review-pi/skills/agent-review/SKILL.md)
   completely.
3. Follow that pinned package workflow exactly. This loader grants no extra
   authority and does not replace the package's review policy.

This compatibility loader exists because the `0.5.0` package's bundled skill
description contains an unquoted YAML colon that Pi `0.84.0` cannot parse. It
must be removed once a later reviewed package pin exposes `skill:agent-review`
directly and `just pi-smoke` proves the replacement.
