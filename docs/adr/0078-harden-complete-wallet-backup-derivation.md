# ADR-0078: Harden complete-wallet backup derivation

- Status: Accepted
- Date: 2026-08-18
- Blueprint source: Sections 3, 7, 9–13, 16–18, and 21
- Prototype source: `midnight-ledger` commit `074b1a4bccbfee1740ee188374b606a022ecef42`, `mobile-bench/wallet-core/src/store/backup.rs` and the Dioxus `WalletBackupCard`
- Tracking: issues #2, #33, and #48
- Implementation state: new complete-wallet exports use the strict version-3 KDF policy; version-2 complete-wallet and version-1 custody packages remain readable through an exact read-only compatibility allowlist; physical-device latency and peak-memory release evidence remains issue #33 work

## Context

ADR-0074 selected Argon2id at 19,456 KiB, two iterations, and one lane for
the first custody-only package. That is the current OWASP minimum Argon2id
password-storage configuration, but it became an inadequate product policy when
ADR-0076 reused the same envelope for one user-selected file containing the
wallet root, every non-derivable key, DID records, account associations, and up
to 64 MiB of credential state. Anyone who obtains that file can test a
human-chosen recovery secret offline, and compromise yields the complete wallet
rather than one authentication record.

The envelope already authenticates its KDF fields, but accepting arbitrary
header parameters would let an attacker request excessive memory or CPU before
authentication. Raising the constants under the existing complete-wallet
version would also hide a material compatibility and resource-policy change.
Existing version-2 files must remain recoverable.

## Decision

New complete-wallet exports use `OXIDBAK1` format version 3 and this fixed
policy:

- Argon2id v1.3 with 65,536 KiB memory, three iterations, one lane, a random
  16-byte salt, and a 32-byte output;
- XChaCha20-Poly1305 with a random 24-byte nonce; and
- the exact version, algorithm identifiers, KDF parameters, salt, nonce, and
  ciphertext length as authenticated associated data.

The adapter maps each readable format version to exactly one KDF policy before
derivation:

| Envelope | Purpose | Argon2id policy | Write policy |
| --- | --- | --- | --- |
| version 1 | legacy custody-only package | 19,456 KiB, t=2, p=1 | retained only by the legacy custody codec |
| version 2 | legacy complete-wallet package | 19,456 KiB, t=2, p=1 | read-only through the public complete-wallet API |
| version 3 | current complete-wallet package | 65,536 KiB, t=3, p=1 | all new complete-wallet exports |

Unknown versions, cross-version KDF tuples, changed algorithm identifiers,
invalid lengths, and trailing data fail before Argon2id. The decoder does not
accept a numeric range or derive directly from attacker-selected work factors.
Changing a version or work factor on an existing package also invalidates its
AEAD associated data.

The 64 MiB / three-iteration policy is an Oxid defense-in-depth choice for a
rare, explicitly initiated, high-value offline export. It is deliberately
stronger than the OWASP minimum rather than being represented as a separate
OWASP recommendation. Export and recovery already execute on ADR-0077's native
worker boundary. Allocation failure remains a redacted, fail-closed operation
failure.

## Security, compatibility, and resource consequences

- New files materially increase the cost of each offline recovery-secret guess.
- Existing version-2 complete-wallet files and version-1 custody-only files
  remain recoverable; opening them does not silently rewrite or relabel them.
- Older Oxid builds do not understand version 3. Users must retain or update to
  a build that implements this decision before relying on a new export.
- The strict version-to-policy map bounds pre-authentication KDF work and avoids
  a header-controlled memory-exhaustion path.
- KDF memory is additional to encrypted document and decoded archive memory.
  Representative physical iOS and Android latency, peak-memory, interruption,
  low-memory, and thermal evidence remains a release gate under issue #33. The
  stronger policy is not evidence that those device gates passed.
- Recovery secrets, derived keys, plaintext archives, and KDF failures remain
  absent from ordinary UI, logging, headless, clipboard, and link surfaces.

## Rejected alternatives

- Keeping the minimum policy for complete-wallet files was rejected because it
  underweights the offline, portable, all-authority impact of the archive.
- Accepting any parameters above a minimum was rejected because unauthenticated
  work factors become a denial-of-service input and make resource behavior
  unreviewable.
- Removing version-2 reads was rejected because it would strand backups created
  by the first complete-wallet release.
- Raising version-2 parameters in place was rejected because compatibility and
  resource policy deserve an explicit wire-version transition.

## References

- [OWASP Password Storage Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html)
