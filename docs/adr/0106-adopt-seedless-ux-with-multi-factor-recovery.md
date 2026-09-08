# ADR-0106: Adopt a seedless UX with multi-factor recovery

- Status: Proposed
- Date: 2026-09-08
- Source: Midnight Passport vision and standards register at commit
  `639da4629db1ef8a6b763a7f43fa27fef6e5391e`; issue #364
- Related: ADR-0017, ADR-0074 through ADR-0078, ADR-0090, ADR-0105, and
  issue #359
- Implementation state: research only; this record authorizes no provider,
  dependency, custody change, or recovery UI

## Context

A written BIP39 mnemonic is portable and works without a provider, but it is a
hostile default for a mainstream mobile Passport experience. It asks the user
to preserve high-value secret text for years and creates familiar phishing,
screen-capture, transcription, and unsafe-copy failure modes. BIP39 itself
defines a mnemonic as an encoding of computer-generated randomness, not as a
way to turn a user-created or otherwise predictable phrase into a wallet root.

The desired property is therefore a **seedless user experience**, not a
predictable seed. Any root or recovery secret that exists must remain
cryptographically random. An Apple ID, Google account, email address, password,
OAuth token, biometric, or human-chosen phrase must never be used directly to
derive it.

The [Midnight Passport vision][passport-demo] describes a stronger eventual
model than hiding a conventional HD root:

- each device holds independent authority derived behind a passkey;
- devices are peers and can enrol or revoke other devices;
- the account contract retains commitments, an authorization epoch, and replay
  state rather than user secrets or identity;
- losing every device invokes a guardian quorum that attaches a fresh device
  and atomically retires old devices and grants; and
- the account, address, name, balances, and attestations survive authority
  rotation.

Its October demonstration deliberately takes a managed shortcut: a passkey
authenticates to a partner-operated MPC committee. The stated standards path is
on-device passkey-derived authority plus BUSS/ANARKey guardian recovery. Oxid
must preserve that distinction instead of relabelling managed recovery as
self-custody.

Oxid currently also supports direct Midnight HD-root assets. ADR-0017 keeps
root and key operations behind opaque native custody. ADR-0074 through ADR-0078
provide an authenticated complete-wallet archive; ADR-0090 presents explicit
create or restore; ADR-0105 admits one owner root for a narrow PreProd profile.
Those assets cannot become seedless merely by putting the same root behind a
cloud login. They require a compatibility bridge until authority can rotate at
the account layer.

## Decision drivers

The decision must tolerate a stolen or lost device, factory reset, platform-key
invalidation, cloud-account takeover or suspension, provider outage, server
database disclosure, compromised minority of guardians, phishing, malicious
factor enrolment, rollback of an old backup, and correlated household-device
loss. An account identifier, cloud session token alone, stolen locked device,
one guardian key, or stored ciphertext must each be insufficient to spend.

The following product properties are mandatory:

1. normal creation and use do not require viewing or transcribing a mnemonic;
2. an already-enrolled device can sign when recovery services are unavailable;
3. no single named cloud, wallet, or MPC provider is the only survivable
   recovery path;
4. recovery preserves the same authority-bearing account where the network
   model permits it, rather than silently creating a new wallet;
5. provider and platform capabilities are truthful, detectable, and replaceable
   behind Oxid-owned ports; and
6. mnemonic or raw-root export remains an advanced interoperability escape
   hatch until a portable replacement has physical-device evidence.

## Options considered

| Option | Recovery and portability | Principal risk | Decision |
| --- | --- | --- | --- |
| BIP39 mnemonic or raw root | Provider-independent, cross-platform, offline | User-visible bearer secret is easily phished, copied, or lost | Retain only as an advanced fallback during migration |
| Device Secure Enclave or Android Keystore | Strong local non-exportability and user-presence policy | Device loss, passcode reset, biometric changes, and algorithm limits make it non-portable | Required local protection; never the only recovery factor |
| Synced passkey assertion | Excellent phishing-resistant account authentication | Authentication alone neither produces a Midnight/Jubjub key nor prevents the auth service from becoming recovery authority | Use for authentication and ceremony authorization, not as sole custody |
| WebAuthn PRF wrapping | A stable credential-scoped output can unwrap an encrypted random secret wherever the passkey and PRF are available | RP/provider binding, uneven native/provider support, synced-credential semantics, and total passkey loss | Preferred first mnemonic-optional convenience factor, with independent fallback |
| Apple/Google account backup | Smooth restore inside one platform ecosystem | Account takeover, suspension, keychain reset, policy drift, and weak cross-platform portability | Optional transport for Oxid-encrypted data only |
| Multiple peer devices | Removes a primary-device bottleneck and permits surviving-device enrolment/revocation | Correlated loss or compromise of all devices | Core recovery layer |
| Guardian or social recovery | Provider-neutral total-loss recovery with a quorum | Availability, collusion, coercion, social engineering, and ceremony complexity | Strategic Passport-aligned total-loss path after cryptographic review |
| Managed threshold/MPC signing | Excellent consumer UX and no complete key at one server | Provider availability and metadata, IAM recovery, committee compromise, operational burden, and curve/protocol mismatch | Optional, explicitly named managed profile only |
| Contract-level authority rotation | Recovery attaches fresh authority while assets retain one stable account | Contract and recovery-circuit correctness; settlement requires the network | Target Passport architecture |

### Passkeys and PRF

Passkeys are phishing-resistant, relying-party-bound credentials. Apple and
Google can synchronize passkeys through their credential ecosystems, while the
FIDO Alliance is standardizing credential exchange. Synchronization is a
valuable recovery factor but introduces an availability and account-recovery
dependency that Oxid must disclose rather than treat as chain-only authority.

An ordinary WebAuthn assertion does not expose its private P-256 key and cannot
be treated as a general Midnight signer. The WebAuthn Level 3 `prf` extension
does support a credential-scoped pseudorandom output intended for symmetric-key
derivation. Where a native platform and selected credential provider prove
support, Oxid may use it as follows:

```text
passkey PRF output
  -> HKDF(domain, account id, network id, envelope version, factor id)
  -> key-encryption key
  -> unwrap random envelope data-encryption key
  -> decrypt random wallet root and protected recovery state
```

Oxid must not derive the wallet root directly from the PRF output. Wrapping a
separate random data-encryption key permits passkey rotation, independent
factors, algorithm migration, and revocation without changing derived wallet
accounts. Each factor receives a distinct authenticated wrap. The recovery
policy must say whether those wraps are alternative paths or shares of a
threshold; the UI must never imply quorum protection for a one-of-many setup.
Unsupported PRF, provider changes, or credential loss fail over to another
enrolled path; they must never fall back to a weak password-derived root.

### Platform and cloud facilities

Secure Enclave and Android Keystore/KeyMint remain device protection, not
backup. Secure Enclave supports only a constrained set of generated keys and
cannot import an arbitrary Jubjub, Ed25519, or wallet root. Android
authentication-bound keys may be permanently invalidated after security-state
changes. A local hardware-backed key may wrap a device-local copy and gate use,
but its loss must not destroy the account.

Synchronizable Keychain items, CloudKit encrypted fields, Android Auto Backup,
and Android Restore Credentials can improve reinstall or same-ecosystem
migration. Oxid may hand them only an already encrypted, versioned envelope or
opaque recovery credential. It must not rely on accidental application backup,
upload native-vault ciphertext that is deliberately device-bound, or make a
single platform account the only recovery path.

### MPC and threshold custody

Threshold cryptography can prevent one participant from holding the complete
signing key, but it adds distributed-key generation, networking, availability,
upgrade, audit, incident-response, compliance, and migration obligations.
Commercial embedded-wallet products demonstrate convenient device,
authentication, and recovery shares, but their recovery trust and export rules
remain provider-specific.

Midnight compatibility is also not implied by the term MPC. Passport uses
in-circuit Schnorr over Jubjub. RFC 9591 standardizes FROST suites for other
groups, not Jubjub. A provider must pass an exact MIP-0013-compatible Jubjub
interoperability and provider-loss exercise before admission. Managed MPC may
serve a demo or separately disclosed managed-custody product, but cannot become
the silent default or the only route to routine signing.

### Guardian recovery

Passport selects the BUSS construction assessed in the ANARKey paper.
Guardians derive shares on demand from keys they already hold; the chain carries
a recovery commitment, public recovery points, and a session nonce. A quorum
reconstructs recovery authority on the replacement device, which registers a
fresh device, rotates recovery material, and bumps the account epoch. The
guardian roster need not be public.

This is the preferred strategic total-loss mechanism because no Apple, Google,
wallet, or MPC operator is indispensable. It still requires review of guardian
availability, collusion, coercion, recovery phishing, notification and delay,
paper-key handling, factor rotation, and the paper's stated formal-model limits.
DeRec or an encrypted-blob profile may remain substitutable behind the same
account recovery seam.

## Proposed architecture

Oxid will pursue two explicitly different layers:

1. **HD-root compatibility.** Keep the randomly generated direct-wallet root
   inside the existing authenticated complete-wallet envelope. Wrap its random
   data-encryption key with device-local hardware protection and two or more
   independent recovery factors. This removes the mnemonic from normal UX
   without pretending that the underlying root ceased to exist.
2. **Passport account authority.** Move long-term spend authority to peer device
   commitments and a stable account contract. Device loss rotates authority;
   total loss invokes guardian recovery. Direct-root assets that cannot yet
   follow that rotation remain explicitly identified migration residue.

The intended capability flow is:

```text
passkey / native user presence
            |
            v
device authority or factor-specific envelope KEK
            |                         stable account contract
Secure Enclave / Keystore             device commitments + epoch
            |                                   ^
            v                                   |
encrypted recovery envelope ---- enrol / revoke / recover
  random root + wallet state
       /           |           \
PRF wrap      encrypted copy   guardian quorum
```

Application and domain code must depend only on provider-neutral capabilities,
for example device user presence, passkey-secret evaluation, recovery-envelope
storage, recovery-factor registry, peer-device enrolment, account-authority
rotation, and guardian recovery coordination. Apple, Google, WebAuthn, cloud,
and MPC concepts stay in Swift/Kotlin/network adapters and composition. The
capability manifest advertises only behavior actually supported by the selected
platform, credential provider, network, and account type.

No raw root, PRF output, data-encryption key, factor key, guardian share, or MPC
share may enter ordinary Dioxus state, URLs, clipboard, logs, diagnostics,
analytics, crash reports, or public profile metadata. Transient Rust, Swift,
and Kotlin buffers remain bounded, redacted, and zeroizing where supported.

## Staged adoption

### Stage 0: specify and preserve compatibility

- Keep the existing versioned authenticated complete-wallet envelope and
  CSPRNG-generated root.
- Separate wallet root, device authority, envelope keys, DID keys, and
  network/account derivation with domain-separated inputs.
- Move mnemonic/raw-root input and export to a clearly labelled advanced path;
  do not remove it yet.
- Define rollback counters, factor identifiers, recovery metadata bounds,
  one-of-many versus threshold policy, and factor add/remove authorization
  before introducing a service.

### Stage 1: local protection and peer devices

- Wrap the local envelope key with reviewed Keychain/Secure Enclave or
  Keystore/KeyMint protection and fresh user presence.
- Permit an authenticated enrolled device to add a peer and revoke a lost one.
- Prove that enrolled-device signing works while every recovery service is
  unavailable.

### Stage 2: passkey convenience

- Implement capability-detected passkey PRF adapters on physical iOS and
  Android devices.
- Add one PRF-derived wrap of the random envelope key; store only encrypted
  envelope bytes and bounded opaque locator/version metadata remotely.
- Require an independent survivable fallback in addition to passkey PRF before
  suppressing the advanced-backup warning.
- Test reinstall, new-device restore, cloud-disabled, provider-change,
  keychain-reset, lockout, cancellation, and cross-platform cases.

### Stage 3: Passport account authority

- Register device commitments, epoch, replay state, and recovery commitment in
  the stable account contract.
- Make recovery add fresh authority and revoke every stale device and grant in
  one atomic network transition.
- Specify how wallet-level DUST, DIDs, credentials, and any remaining
  direct-root assets migrate or recover alongside the account.

### Stage 4: guardian total-loss recovery

- Adopt a reviewed BUSS/Pleiades-compatible ceremony with fresh recovery secret
  and nonce for every guardian-set change.
- Define quorum, delay, notification, cancellation, rate limits, guardian
  replacement, and paper-key UX.
- Keep the guardian roster/social graph off-chain and make transports
  substitutable.

An optional managed-MPC profile requires a separate ADR and issue, explicit
custody disclosure, exact Jubjub compatibility, provider-loss/migration plan,
service-level and audit evidence, and proof that the provider alone cannot
decrypt or spend.

## Consequences

- Mainstream onboarding can become mnemonic-free without weakening root
  entropy or hiding a new custody operator.
- Near-term direct-wallet users remain recoverable through the existing
  encrypted archive while the UI can progressively prefer peer devices and
  passkeys.
- Cross-platform and total-loss guarantees come from independent factors, not
  optimistic assumptions about one synced-passkey ecosystem.
- The long-term Passport model rotates authority instead of reconstructing an
  obsolete device secret or moving assets to a new account.
- Cloud storage holds ciphertext and bounded metadata only. Provider compromise
  or database disclosure alone is insufficient to spend.
- Recovery becomes a security-sensitive state machine requiring downgrade,
  rollback, replay, factor-enrolment, and account-rotation evidence.
- Implementing guardian recovery or Jubjub threshold signing remains blocked on
  protocol and cryptographic review; this Proposed ADR does not approve either.

## Required evidence before acceptance

- Physical iOS and Android PRF capability/support matrix, including third-party
  credential providers, sync/restore behavior, and unsupported fallbacks.
- A versioned envelope/factor threat model proving that one stored ciphertext,
  provider credential, device, or sub-quorum cannot recover spend authority.
- Create, reinstall, new-device, offline-use, provider-outage, cloud-account-
  loss, key-invalidation, factor-removal, rollback, and cancellation tests.
- Exact MIP-0013/Jubjub interoperability before admitting any MPC provider.
- Cryptographic review of BUSS integration, domain separation, parameters,
  guardian rotation, and the ANARKey model limitations.
- A migration inventory for NIGHT, shielded assets, DUST, DIDs, credentials,
  grants, and chain-derived versus non-rebuildable local state.
- UX review proving that “seedless” is not presented as “secretless,” that
  custody/provider dependence is disclosed, and that advanced recovery remains
  usable without making it the first-run burden.

## Rejected alternatives

- **Derive a seed from Apple/Google identity, email, password, or OAuth.** These
  values lack wallet-root entropy, can be recovered or reassigned by another
  authority, and create an offline-guessing or account-takeover root of trust.
- **Derive the wallet root directly from passkey PRF.** This couples all wallet
  addresses to one credential/provider lifecycle and prevents independent
  wrapping-factor rotation.
- **Use platform automatic backup as the custody model.** Device-bound vault
  ciphertext is not portable; silent backup provides neither explicit recovery
  evidence nor provider independence.
- **Call managed MPC self-custody.** Threshold shares reduce one failure mode
  but do not remove provider, IAM, availability, metadata, and migration trust.
- **Remove mnemonic/raw-root recovery immediately.** Current HD-root users need
  a proven portable escape hatch until multi-factor cross-platform recovery has
  physical-device evidence.
- **Merge seed QR and network QR.** Issue #359 correctly keeps secret recovery
  envelopes separate from public connection manifests and protocol routing.

## Open questions

1. Which iOS and Android versions and credential providers expose stable native
   PRF output across passkey synchronization and device restore?
2. Does a synced passkey represent one logical Passport device or must each
   physical device register a distinct credential and commitment?
3. Should encrypted envelopes live in user-selected cloud storage, a blind
   provider-neutral blob store, or both, and how are locators kept unlinkable
   from the on-chain account?
4. Can every current Midnight role rotate under the Passport account, or which
   direct-root assets remain outside account recovery?
5. Are DIDs and VCs restored with wallet authority, independently backed up, or
   reissued under continuity proofs?
6. Which delay and cancellation policy limits cloud-account-takeover recovery
   without making legitimate total loss impractical?
7. Can BUSS guardians safely use existing passkeys or wallet keys without
   deterministic-signature, cross-protocol, or domino-effect hazards?
8. What portable export format remains available while FIDO credential exchange
   adoption is incomplete?

## References

- [Midnight Passport: How it works][passport-demo]
- [Midnight Passport standards register][passport-standards]
- [BIP39: Mnemonic code for generating deterministic keys][bip39]
- [WebAuthn Level 3 PRF extension][webauthn-prf]
- [Apple Passkeys][apple-passkeys] and
  [AuthenticationServices updates][apple-auth-updates]
- [Google passkey supported environments][google-passkeys]
- [FIDO Credential Exchange Specifications][fido-exchange]
- [NIST SP 800-63B authenticator requirements][nist-800-63b]
- [Apple Secure Enclave key protection][secure-enclave] and
  [synchronizable Keychain items][apple-keychain-sync]
- [Android Keystore/KeyMint][android-keystore],
  [Auto Backup][android-backup], and
  [Restore Credentials][android-restore]
- [NIST Threshold Cryptography project][nist-threshold] and
  [RFC 9591: FROST][rfc9591]
- ANARKey/BUSS, IACR ePrint 2025/551, and the
  [Pleiades implementation][pleiades]
- [DeRec protocol repository][derec]
- Managed-wallet examples:
  [Privy user-device wallets][privy],
  [Fireblocks embedded-wallet backup][fireblocks], and
  [Web3Auth MPC][web3auth]

[passport-demo]: https://midnightntwrk.github.io/passport/site/demo.html
[passport-standards]: https://midnightntwrk.github.io/passport/site/standards.html
[bip39]: https://github.com/bitcoin/bips/blob/master/bip-0039.mediawiki
[webauthn-prf]: https://www.w3.org/TR/webauthn-3/#sctn-prf-extension
[apple-passkeys]: https://developer.apple.com/passkeys/
[apple-auth-updates]: https://developer.apple.com/documentation/updates/authenticationservices
[google-passkeys]: https://developers.google.com/identity/passkeys/supported-environments
[fido-exchange]: https://fidoalliance.org/specifications-credential-exchange-specifications/
[nist-800-63b]: https://pages.nist.gov/800-63-4/sp800-63b/authenticators/
[secure-enclave]: https://developer.apple.com/documentation/security/protecting-keys-with-the-secure-enclave
[apple-keychain-sync]: https://developer.apple.com/documentation/security/ksecattrsynchronizable
[android-keystore]: https://source.android.com/docs/security/features/keystore
[android-backup]: https://developer.android.com/identity/data/autobackup
[android-restore]: https://developer.android.com/identity/sign-in/restore-credentials-implementation
[nist-threshold]: https://csrc.nist.gov/projects/threshold-cryptography
[rfc9591]: https://www.rfc-editor.org/info/rfc9591/
[pleiades]: https://github.com/input-output-hk/arc-pleiades
[derec]: https://github.com/derecalliance/protocol
[privy]: https://docs.privy.io/security/wallet-infrastructure/advanced/user-device
[fireblocks]: https://developers.fireblocks.com/docs/embedded-wallet-backup-and-recovery
[web3auth]: https://web3auth.io/mpc.html
