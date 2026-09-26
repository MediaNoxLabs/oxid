# Portable custody backup KDF policy

Custody-only exports use `OXIDBAK1` version **6**, adopting the fixed policy
reviewed for complete-wallet backups in
[ADR-0078](../../adr/0078-harden-complete-wallet-backup-derivation.md).
This changes the authenticated envelope, not the custody payload schema.
It does not establish production or physical-device readiness.

## Exact policies and authenticated allocation boundary

| Version | Payload | Argon2id v1.3 policy | Public write policy |
| --- | --- | --- | --- |
| 1 | Legacy custody, raw development root | 19,456 KiB, t=2, p=1 | Read-only |
| 2 | Legacy complete wallet | 19,456 KiB, t=2, p=1 | Read-only |
| 3 | Legacy complete wallet | 65,536 KiB, t=3, p=1 | Read-only |
| 4 | Custody, explicitly typed root | 19,456 KiB, t=2, p=1 | Read-only |
| 5 | Complete wallet, explicitly typed root | 65,536 KiB, t=3, p=1 | Current complete-wallet export |
| 6 | Custody, explicitly typed root | 65,536 KiB, t=3, p=1 | Only current custody export |

Both policies derive a 32-byte key using a fresh 16-byte salt. Encryption uses
XChaCha20-Poly1305 and a fresh 24-byte nonce. The whole header, including the
version, algorithm identifiers, exact KDF tuple, salt, nonce, and ciphertext
length, is authenticated associated data.

The entry point's version allowlist and exact work-factor/algorithm matching
reject unknown versions, wrong payload families, mismatched parameters, and
invalid lengths before deriving a key. Payload-schema validation occurs only
after successful authentication and decryption. The decoder never allocates an
Argon2 arena from arbitrary header values. A changed v6 header claiming v1/v4
with unchanged strong parameters is invalid before derivation; changing both
the version and parameters into an allowed legacy tuple still fails AEAD
authentication. Substitution of the same-policy complete-wallet version also
fails authentication. No fallback retries weaker policies.

The 64 MiB arena is additional to document, plaintext, and application memory.
It raises the cost of each offline guess but does not make a weak recovery
secret safe. Keep recovery secrets, derived keys, and plaintext out of logs.

## Compatibility, expiry, and migration

- Custody v1/v4 remains readable through an exact allowlist, including v4's
  64-byte BIP-39 root and raw development root. New exports never emit v1/v4.
- There is no automatic expiry date for these legacy reads. Removing them
  requires a separately reviewed migration decision that does not strand
  recovery files. Unknown future formats fail closed.
- Opening an old file does not modify it or silently strengthen it. Explicitly
  re-export with a v6-capable build and verify recovery before replacing an old
  backup. Older builds cannot read v6; retain access to a compatible build.
- Copies of old files retain their original offline-attack cost permanently.
  Re-export cannot revoke leaked copies or retroactively increase that cost.
  If compromise is suspected, re-export alone is not a custody-rotation remedy.

### Shipping-history evidence

Repository history introduces v4 in
`d25b15d1c9a3d2b54403d2d91f9ec4d4eaf5bb31`, dated 2026-09-08,
`feat(midnight): support explicit wallet roots (#360)`. The fetched
`origin/milestone-0.2.0` contains that commit. The 2026-09-26 GitHub inventory
reported no releases and one remote tag, `foundation-0`; that tag does not
contain the v4 commit. This bounds the repository-hosted release evidence, but
does **not** prove that no untagged binary or v4 file reached a user. Treat any
existing v1/v4 file as needing the re-export guidance above.

Reproduce the public inventory and ancestry check with:

```sh
gh release list -R MediaNoxLabs/oxid --limit 100 --json tagName
gh api repos/MediaNoxLabs/oxid/tags --paginate \
  --jq '.[] | [.name,.commit.sha] | @tsv'
git merge-base --is-ancestor \
  d25b15d1c9a3d2b54403d2d91f9ec4d4eaf5bb31 \
  c7c5b5b8004bccbee8a0f3afcbb22b924d790fff
```

The last command exits 1 because `foundation-0` does not contain the v4
commit. A future inventory must record any newer release or tag rather than
reusing this dated conclusion.

## Bounded host evidence and outstanding mobile qualification

On 2026-09-26, a single-threaded, unoptimized Rust test on macOS arm64,
Apple M2 Max (96 GiB RAM), sealed and opened one small synthetic custody vault
with v6. `/usr/bin/time -l` around the compiled test executable reported:

- wall time: **2.99 s** (2.93 s user, 0.05 s system);
- maximum RSS: **70,402,048 bytes**;
- peak footprint: **68,419,944 bytes**;
- swaps: **0**; host swap before/after: **0 MiB**.

The measured binary was built by the pinned debug toolchain. Re-run the same
functional assertion, including a fresh compile when needed, with:

```sh
CARGO_BUILD_JOBS=2 cargo test -p oxid-adapter-backup-portable --locked --lib \
  tests::custody_exports_only_v6_with_the_strong_policy -- \
  --exact --test-threads=1
```

The bounded attempt's 30-second test-execution timeout and functional
round-trip assertion passed. Compilation/cache state is outside the reported
binary-only timings. This is a host smoke, not a release benchmark,
maximum-payload budget, low-end mobile measurement, or separate export/recovery
latency measurement. No device memory threshold is claimed to have passed.

Supported low-end iOS/Android qualification is **outstanding**, explicitly
outside this fast-line implementation. Before release, the supervisor must
identify the minimum supported device classes and approve peak-memory and
elapsed-time pass/fail thresholds *before* measuring optimized builds. Record
OS/device class, exact build, payload size, export and recovery latency, peak
memory, low-memory/interruption/thermal outcomes, and pass/fail without secrets.
ADR-0078's physical-device release gate remains open; host success cannot satisfy
it. The supervisor authorized stopping new weak exports without claiming that
mobile acceptance criterion complete.
