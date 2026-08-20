# Browser entropy backends and the wasm32 target/feature split

The Tier-2 browser target (`wasm32-unknown-unknown`, ADR-0012) has no
operating-system entropy syscall. `getrandom` therefore refuses to compile for
it unless the build explicitly selects the JavaScript backend, which calls
`Crypto.getRandomValues` through `wasm-bindgen`. Selecting that backend is a
whole-graph decision, and it must never leak into the Tier-1 Android and iOS
graphs, so it is expressed with target-scoped configuration only.

## Three getrandom majors resolve in the browser graph

| Major | Reaches the graph through | Selector required for wasm32 |
| --- | --- | --- |
| 0.2.17 | `midnight-curves` and `rand_core` 0.6 in the Midnight proof stack | `js` crate feature |
| 0.3.4 | workspace pin, used directly by `adapters/platform-system`, `adapters/storage-credential-json`, and `adapters/vc-midnight` | `wasm_js` crate feature **and** `--cfg getrandom_backend="wasm_js"` |
| 0.4.3 | `crypto-common` in the RustCrypto stack (`chacha20poly1305`, `digest`, `ed25519-dalek`) | `wasm_js` crate feature |

The 0.3 line is the only one that needs a compiler cfg in addition to a
feature. Upstream split the two deliberately: the feature makes the backend
available, and the cfg is the build's explicit statement that the artifact will
only ever run where a Web Crypto context exists. Enabling the feature alone
fails with a `compile_error!` that says as much.

## Where each selector lives

- `.cargo/config.toml` carries `--cfg getrandom_backend="wasm_js"` under
  `[target.wasm32-unknown-unknown]`. Scoping it to the triple keeps the native
  desktop and Tier-1 mobile builds on their operating-system backends; those
  builds never see the flag.
- `apps/oxid/Cargo.toml` declares the 0.2 and 0.4 majors under
  `[target.'cfg(target_arch = "wasm32")'.dependencies]` with their JavaScript
  backend features enabled. Cargo can only turn on a transitive crate's feature
  from a manifest that names the crate, and the browser shell is the one graph
  root that owns the target choice, so the two indirect majors are selected
  there rather than inside an adapter that never names them. Feature
  unification then applies the choice to every copy in the wasm32 graph, and
  the `cfg(target_arch = "wasm32")` scope keeps `wasm-bindgen` and `js-sys` out
  of the Android, iOS, and desktop graphs entirely.
- The workspace `getrandom` pin keeps `wasm_js` enabled for the 0.3 line. That
  feature only adds target-scoped `wasm-bindgen`/`js-sys` dependencies inside
  `getrandom` itself, so it is inert for native builds.

## Exit strategy

The adapter boundary is unchanged: application and domain code still see only
`RandomPort` from `crates/platform/ports`, and `adapters/platform-system`
remains the single implementation. Replacing `getrandom` means replacing that
adapter and deleting the two selectors above; no core type moves.

Any dependency bump that introduces a fourth `getrandom` major must add its
browser backend selector in the same two places, or the browser check fails
with an explicit `compile_error!` rather than silently degrading entropy.
