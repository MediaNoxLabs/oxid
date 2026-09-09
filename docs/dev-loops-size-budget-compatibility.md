# Oxid size-budget compatibility

The pinned `dev-loops@1.0.2` evaluator recognizes JavaScript and TypeScript only. Oxid routes only `dev-loops gate size-budget` and `dev-loops pr ready-for-review` through a repository-owned adapter that measures Rust, Kotlin, and Swift source by deterministic `git diff --numstat` LOC. Recognized test paths retain the configured test discount; docs, CI, configuration, generated paths, and lockfiles are excluded. Other unknown source-like paths retain upstream's majority-unclassified fail-closed rule.

This slice does not alter installed package files, absolute thresholds, or any other upstream gate. Remove it when the pinned upstream evaluator natively supports Rust, Kotlin, and Swift with the same conservative source/test classification and the two wrapper routes can return to direct upstream dispatch.
