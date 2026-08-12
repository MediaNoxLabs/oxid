# qrcode 0.14 dependency review

- Project: [`kennytm/qrcode-rust`](https://github.com/kennytm/qrcode-rust)
- Selected version: `qrcode` 0.14.1, pinned exactly by the workspace and
  `Cargo.lock`
- License: MIT OR Apache-2.0
- Maintenance/activity: 0.14.1 was published 2024-07-05. The repository is not
  archived and its latest reviewed commit was `c7780e8549ac4fb3da81fdf7d7f010a27db78c0f`
  from 2025-08-25. This is a small, stable encoder rather than an active wire or
  cryptographic protocol client.
- Security/audit evidence: no independent Oxid audit and no security policy was
  visible in the repository at review time. Cargo advisory, source, and license
  gates remain mandatory. Inputs are bounded to validated public addresses, and
  encoder failure produces a local unavailable state rather than a panic.
- Android/iOS/desktop/WASM: pure Rust. Default features are disabled and only
  the SVG renderer is enabled, excluding the raster `image` dependency. Native
  Tier-1 builds and the separate browser target remain required gates.
- Cryptography: none. QR error correction is not authentication, integrity, or
  encryption and must not be presented as such.
- API stability: the crate's types remain private to `crates/ui-dioxus`; core,
  application, composition, and headless APIs expose only the public address.
- Reason selected: the reviewed prototype uses the same crate family to render
  deterministic receive QR codes without JavaScript, files, or native bridge
  code.
- Alternatives considered: hand-written QR encoding (unnecessary parser and
  matrix risk), browser JavaScript libraries (violates the controlled-edge
  preference), raster generation through `image` (larger graph), and a native
  camera/QR SDK (wrong boundary for public receive-code rendering).
- Adapter boundary: Dioxus presentation only. Camera scanning remains behind a
  future `QrScannerPort`; clipboard/share remain native platform adapters.
- Exit strategy: replace the private render helper with another pure encoder or
  a reviewed native display adapter without changing wallet core or
  application types.
