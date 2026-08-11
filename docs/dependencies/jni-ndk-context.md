# jni 0.21 and ndk-context 0.1 dependency review

- Projects: [jni-rs](https://github.com/jni-rs/jni-rs) and
  [android-ndk-rs](https://github.com/rust-windowing/android-ndk-rs)
- Selected versions: jni 0.21.1 and ndk-context 0.1.1, exactly pinned for the
  Android target and in `Cargo.lock`
- License: MIT OR Apache-2.0
- Maintenance/activity: established Rust Android ecosystem crates already
  present in the pinned Dioxus/Tao/Wry mobile graph; changes remain explicit
  lockfile reviews
- Security/audit evidence: no independent Oxid audit; `cargo audit` plus source,
  license, advisory, Android build, and emulator smoke gates apply
- Android: target-specific dependencies used to ask the application `Context`
  for its durable internal files directory
- iOS, desktop, and WASM/web: not compiled as direct dependencies of the profile
  adapter on these targets
- Cryptography: none; these crates select an application data path and do not
  implement protected key storage
- API stability: both are pre-1.0. Their use is isolated to one small
  Android-specific path function and exact versions are pinned.
- Reason selected: Android application processes do not reliably expose `HOME`,
  while the Rust temporary directory intentionally resolves to cache and is not
  durable profile storage
- Alternatives considered: persisting under Android cache, deriving internal
  paths from package names, increasing the minimum Android release, or adding a
  Java/Kotlin storage bridge. Cache is evictable, derived paths are brittle, an
  OS-version increase is unnecessary, and a custom bridge duplicates the
  existing runtime context.
- Adapter boundary: Android-only dependencies of
  `crates/adapters/storage-json`; core and incoming adapters remain independent
- Unsafe boundary: two pointer wrappers convert runtime-owned JavaVM/Context
  handles supplied by initialized `ndk-context`; their scopes and invariants are
  documented at the call site, and all JNI operations after conversion use the
  checked `jni` API
- Exit strategy: replace this path lookup with a safe platform-path port or
  native storage adapter without changing `WalletProfileRepository`
