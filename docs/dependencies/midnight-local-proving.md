# Midnight local proving dependency review

- Reviewed: 2026-08-12
- ADR: [ADR-0028](../adr/0028-keep-midnight-proof-witnesses-local.md)
- Scope: native private DUST proof generation and interoperability measurement

## Selected source and versions

`midnight-zkir 2.1.0` is a direct, native-only dependency from
`https://github.com/midnightntwrk/midnight-ledger.git` at the full immutable
revision `d9414884db9da9e9b1f6f3a7f742d79a5732f817`. Default features are
disabled. It supplies `IrSource` and `LocalProvingProvider` compatible with the
same ledger transaction types used by the submission adapter.

The resolved graph uses the published `midnight-proofs 0.7.3`,
`midnight-circuits 6.3.0`, and `midnight-zk-stdlib 1.3.0` releases already
selected transitively by the pinned ledger proving graph. No unpublished crate
is referenced by a registry version or local path. No mutable branch, tag, or
patch source is used.

## License, maintenance, and security

The direct package is Apache-2.0 under the reviewed ledger repository. The
transitive proof crates are Apache-2.0 or MIT OR Apache-2.0 as recorded in the
standalone-submission review. Existing bounded RustSec exceptions for the
upstream graph remain in `docs/security/advisory-exceptions.md`; local proving
does not add another exception.

Proof parameters and DUST circuit material are public but security-critical.
The adapter accepts only fixed official HTTPS paths and hashes, ignores ambient
proxies, streams into bounded owner-only temporary files, authenticates before
atomic installation, and re-authenticates cache hits. The cache rejects
symlinks, non-files, more than 32 entries, or more than 8 MiB total. It is not a
credential or secret store, but platform composition must still locate it in an
app-private cache directory.

## API stability and adapter boundary

`LocalProvingProvider` and `IrSource` are used only inside
`crates/adapters/midnight`. Oxid-owned application/domain APIs expose neither
proof-system nor ledger types. A source update must review the DUST IR k value,
row count, file hashes, proof encoding, mobile latency/RSS, licenses, and
advisories together.

The local prover is serialized per adapter and runs on the named submission
worker. Cancellation is cooperative at safe pre-broadcast boundaries because
the upstream Halo2 future cannot currently be interrupted mid-proof.

## Target evidence and exit strategy

The release interoperability harness compiled and ran on macOS arm64, an arm64
iPhone simulator, and an arm64 Android emulator. It produced a valid k=13 DUST
proof with 5,646 modeled rows, a 2,912-byte proof, and a sealed transaction that
round-tripped through the pinned tagged codec. Exact 2026-08-12 latency, RSS,
cache, and binary measurements are recorded in ADR-0028.

Replace the implementation behind the same adapter boundary if the official
project publishes a smaller or interruptible mobile prover. Do not change k,
artifact sources, or hashes as a routine dependency refresh.
