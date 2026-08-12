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
- [directories 6](directories-6.md)
- [getrandom 0.3](getrandom-0.3.md)
- [jni 0.21 and ndk-context 0.1](jni-ndk-context.md)
- [qrcode 0.14](qrcode-0.14.md)
- [chacha20poly1305 0.11](chacha20poly1305-0.11.md)
- [RustCrypto development signing stack](rustcrypto-development-signing.md)
- [Serde and serde_json](serde-json.md)
- [url 2.5](url-2.5.md)
- [Tokio and tokio-tungstenite](tokio-tungstenite-0.30.md)
- [Midnight Git sources](midnight-git-sources.md)
- [Midnight ledger transaction packages](midnight-ledger-8.2.md)
- [Midnight standalone submission stack](midnight-standalone-submission.md)
- [Midnight local proving](midnight-local-proving.md)
- [Midnight DID resolution](midnight-did-resolution.md)
