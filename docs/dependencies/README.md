# Dependency reviews

Significant runtime dependencies receive a review before they become part of a
production-facing adapter. Reviews follow the template in the root blueprint:
version, license, maintenance, security evidence, target support, cryptography,
API stability, rationale, alternatives, adapter boundary, and exit strategy.

The Cargo lock file pins the resolved graph. Automated updates target
`integration` through the repository's GitHub default-branch authority, and
changes must pass advisory, license, source, build, and test gates.

Current reviews and source policies:

- [Dioxus 0.7](dioxus-0.7.md)
- [Native mobile bridge](native-identity-ingress.md)
- [directories 6](directories-6.md)
- [getrandom 0.3](getrandom-0.3.md)
- [jni 0.21 and ndk-context 0.1](jni-ndk-context.md)
- [qrcode 0.14](qrcode-0.14.md)
- [chacha20poly1305 0.11](chacha20poly1305-0.11.md)
- [argon2 0.5](argon2-0.5.md)
- [RustCrypto development signing stack](rustcrypto-development-signing.md)
- [Serde and serde_json](serde-json.md)
- [url 2.5](url-2.5.md)
- [Tokio and tokio-tungstenite](tokio-tungstenite-0.30.md)
- [Midnight Git sources](midnight-git-sources.md)
- [Midnight ledger transaction packages](midnight-ledger-8.2.md)
- [Midnight standalone submission stack](midnight-standalone-submission.md)
- [Midnight local proving](midnight-local-proving.md)
- [Midnight Compact Digital Passport presentation](midnight-compact-presentation.md)
- [Midnight DID resolution](midnight-did-resolution.md)
- [Tier-2 browser entropy backends](wasm-web-entropy.md)
- [rmcp](rmcp.md) — proposed MCP SDK for the production agent surface (ADR-0099); not yet adopted.

## Automated dependency pull requests

Two bots operate on this repository with deliberately separate jobs. Both
inherit `integration` from GitHub's default branch; neither configuration
repeats an explicit base. Renovate's absent `baseBranchPatterns` preserves its
default-branch behavior. Dependabot's absent `target-branch` is
security-relevant: adding that key disables security updates for the configured
ecosystem even when it names the default branch.

- **Renovate** (`renovate.json`) owns routine version bumps. It understands
  the exact `=x.y.z` pinning convention, batches related crates into grouped
  pull requests, and observes a seven-day cooling window before proposing a
  release.
- **Dependabot** owns security updates only. It reacts to advisories against
  the committed lockfile, so it can propose a bump before Renovate's cooling
  window would.

Overlapping proposals for the same crate are therefore expected, not
duplication: prefer the Renovate pull request unless the Dependabot one is
addressing an advisory, in which case take the security bump first.

Automation configuration changes affect newly created pull requests, not open
ones. Dependabot PRs [#138](https://github.com/MediaNoxLabs/oxid/pull/138) and
[#139](https://github.com/MediaNoxLabs/oxid/pull/139) still target the retired
`develop` branch and cannot land under ruleset `21481544`. Close those stale
PRs after the default-branch change lands and allow Dependabot to recreate any
update that remains applicable; do not use the old PRs as delivery
or validation evidence.

Both bots are configured to produce a conventional type and the `deps` scope.
Their GitHub-controlled branch names are exempt from the issue-branch grammar,
and their generated commits are exempt from DCO certification only when both
the PR actor and commit author match the closed bot policy. PR title/body,
scope, contribution labels, and GitHub-verified OpenPGP rules still apply. A
generated update that cannot meet them must be recreated as a signed,
issue-backed human or agent contribution; the gate is not waived. Every product
gate (CI, Quality, Scan) continues to run unchanged.
