# Tokio and tokio-tungstenite dependency review

- Projects: [Tokio](https://github.com/tokio-rs/tokio) and
  [tokio-tungstenite](https://github.com/snapview/tokio-tungstenite)
- Selected versions: Tokio 1.53.1, tokio-tungstenite 0.30.0, and Rustls
  0.23.43, exactly pinned in the workspace and `Cargo.lock`
- Licenses: MIT for Tokio and tokio-tungstenite; Rustls is available under
  Apache-2.0, ISC, or MIT; the Mozilla CA certificate data in WebPKI Roots is
  CDLA-Permissive-2.0. That permissive data license is narrowly allowlisted in
  `deny.toml` for the reviewed TLS root dependency.
- Maintenance/activity: Tokio is an established 1.x async runtime;
  tokio-tungstenite 0.30.0 is the current crates.io release reviewed on
  2026-08-12. Updates remain explicit lock-file reviews.
- Security/audit evidence: no independent Oxid audit. The workspace runs
  RustSec advisory checks plus license, source, build, and test gates on every
  change. Network responses are treated as untrusted and decoded through
  bounded frames, timeouts, event/UTXO counts, and typed validation.
- Android support: native Tokio TCP and Rustls/WebPKI transport; verified by
  the Tier-1 Android compile and emulator gates
- iOS support: native Tokio TCP and Rustls/WebPKI transport; verified by the
  Tier-1 iOS compile and simulator gates
- Desktop support: supported by the same native transport
- WASM/web support: deliberately not selected. The dependency is target-gated
  outside `wasm32`; a browser WebSocket adapter will require a separate origin
  and capability review.
- Cryptography: no wallet cryptography. The selected
  `rustls-tls-webpki-roots` feature provides TLS transport authentication and
  avoids OpenSSL/native-TLS linkage. Oxid explicitly selects and installs the
  pinned Rustls Ring provider so `wss` cannot reach a missing-provider runtime
  panic. The provider never receives wallet key material.
- API stability: Tokio has a stable 1.x compatibility policy;
  tokio-tungstenite remains pre-1.0 and is isolated behind the Midnight
  adapter transport.
- Reason selected: the official Midnight indexer v4 account-read protocol is a
  `graphql-transport-ws` subscription. This pair provides native async sockets,
  WebSocket framing, subprotocol negotiation, ping/pong, TLS, and bounded
  timeouts without importing the TypeScript Wallet SDK.
- Alternatives considered: querying GraphQL over HTTP, blocking
  `tungstenite`, native-TLS, embedding the prototype transport, and a foreign
  JavaScript runtime. The v4 unshielded account operation is subscription-only;
  blocking transport would violate the UI-thread rule; native-TLS adds
  platform linkage; and prototype/JavaScript reuse would cross the accepted
  adapter boundary.
- Adapter boundary: native-only dependency of `crates/adapters/midnight` and a
  test-only dependency of the executable headless fixture. Domain and
  application crates remain network-runtime independent.
- Exit strategy: replace the transport behind `MidnightAccountSource` while
  preserving the pinned GraphQL document, Oxid-owned snapshots, startup
  configuration contract, and headless methods
