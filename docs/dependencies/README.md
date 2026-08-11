# Dependency reviews

Significant runtime dependencies receive a review before they become part of a
production-facing adapter. Reviews follow the template in the root blueprint:
version, license, maintenance, security evidence, target support, cryptography,
API stability, rationale, alternatives, adapter boundary, and exit strategy.

The Cargo lock file pins the resolved graph. Automated updates target
`develop`, and changes must pass advisory, license, source, build, and test
gates.
