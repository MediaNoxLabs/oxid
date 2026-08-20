# Dependency reviews

Significant runtime dependencies receive a review before they become part of a
production-facing adapter. Reviews follow the template in the root blueprint:
version, license, maintenance, security evidence, target support, cryptography,
API stability, rationale, alternatives, adapter boundary, and exit strategy.

The Cargo lock file pins the resolved graph. Automated updates target
`develop`, and changes must pass advisory, license, source, build, and test
gates.

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

## Automated dependency pull requests

Two bots operate on this repository with deliberately separate jobs:

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

Neither bot can satisfy this repository's contribution gates — they cannot add
a DCO `Signed-off-by` trailer to generated commits, nor author a title and body
matching the conventional-commit scopes and pull-request template. The DCO and
pull-request validation workflows therefore skip pull requests authored by
`dependabot[bot]` and `renovate[bot]`. Every other gate (CI, Quality, Scan) runs
unchanged: the checks that verify the *change* still apply in full, and only the
checks that verify *authorship formalities* are skipped.
