# ADR-0106: Adopt a seedless UX with passkey recovery

- Status: Proposed
- Date: 2026-09-08
- Source: issue #364 and source-backed wallet-recovery research
- Related: ADR-0017, ADR-0074 through ADR-0078, ADR-0090, ADR-0105, and
  issues #359/#364
- Implementation state: research only; this record authorizes no provider,
  dependency, custody change, or recovery UI

## Context

A written BIP39 mnemonic is portable and provider-independent, but it is a
hostile default for a mainstream mobile wallet. It asks a user to preserve a
high-value bearer secret for years and creates familiar phishing,
screen-capture, transcription, and unsafe-copy failure modes. BIP39 defines a
mnemonic as an encoding of computer-generated randomness, not a way to turn a
user-created or otherwise predictable phrase into a wallet root.

The desired property is a **seedless user experience**, not a predictable seed.
The wallet secret remains cryptographically random. An Apple or Google account
identifier, email address, password, OAuth subject or token, biometric, or
human-chosen phrase must never derive it directly.

Authentication, synchronization, recovery, key protection, and blockchain
signing are separate capabilities:

- an Apple or Google account can synchronize and recover a passkey;
- a passkey provides relying-party-bound, phishing-resistant authentication;
- the WebAuthn `prf` extension may produce credential-scoped secret material;
- platform hardware can protect a device-local wrapping key; and
- the wallet still owns its random secret, derivation, signing, and encrypted
  recovery envelope.

Oxid already supports direct Midnight HD-root assets. ADR-0017 keeps root and
key operations behind opaque native custody. ADR-0074 through ADR-0078 provide
an authenticated complete-wallet archive; ADR-0090 presents explicit create or
restore; ADR-0105 admits one owner root for a narrow PreProd profile. The first
seedless iteration must preserve those boundaries instead of replacing the
mnemonic with an undeclared cloud or wallet provider.

## Decision

Start with one deliberately small path:

1. WebAuthn Level 2-compatible passkey registration and authentication;
2. passkey synchronization through the user's Apple or Google credential
   ecosystem;
3. capability-detected use of the WebAuthn Level 3 `prf` extension; and
4. a PRF-derived key-encryption key that wraps Oxid's independently random
   wallet recovery key.

PRF is not part of WebAuthn Level 2. Oxid therefore treats PRF as progressive
enhancement on top of a Level 2-compatible authentication ceremony. A passkey
without PRF can authenticate the user but cannot unlock the wallet envelope
through this path. The UI and capability manifest must expose that difference.

The selected key flow is:

```text
CSPRNG random 32-byte wallet master secret
                 |
                 +--> chain/account derivation --> protected signing custody
                 |
                 +--> authenticated encrypted recovery envelope
                                                    |
passkey PRF output                                 |
  -> HKDF(domain, wallet id, network id,           |
          envelope version, factor id)             |
  -> passkey key-encryption key --------------------+
  -> unwrap random envelope key
  -> decrypt random wallet master secret and state
```

Oxid must not derive the wallet master secret directly from PRF output.
Wrapping a separately generated random envelope key permits passkey rotation,
provider migration, algorithm changes, and additional recovery factors without
changing wallet accounts.

The initial user journey is:

1. create a wallet secret with an OS CSPRNG;
2. create or select a passkey using the platform account;
3. detect PRF support for that exact credential;
4. evaluate PRF after fresh user verification;
5. derive a domain-separated wrapping key with HKDF;
6. encrypt and authenticate the wallet recovery envelope;
7. store only ciphertext plus bounded, opaque version and locator metadata;
8. on a new device, restore the synchronized passkey, evaluate PRF, fetch the
   encrypted envelope, and recover the same wallet; and
9. retain the existing advanced encrypted export until the new path has
   physical-device and total-loss evidence.

Google and Apple are passkey synchronization and account-recovery providers in
this model. Neither is the source of wallet entropy, the blockchain identity,
or a party that receives plaintext wallet material. Oxid does not assume that
two independently created credentials under the same platform account produce
the same PRF output. Provider migration adds a new wrapper before removing the
old wrapper.

## Provider-neutral boundary

Domain and application code depend on capabilities, not brands:

- require fresh user presence;
- register and authenticate a passkey;
- report whether a specific credential supports PRF;
- evaluate a credential-scoped secret;
- wrap or unwrap an envelope key;
- store or fetch bounded opaque ciphertext; and
- enrol, rotate, remove, and enumerate recovery factors.

Apple, Google, WebAuthn, native-vault, and cloud concepts remain in Swift,
Kotlin, Rust platform, or network adapters. The capability manifest advertises
only behavior demonstrated for the selected platform, credential provider,
network, and profile.

No wallet root, PRF output, envelope key, wrapping key, or plaintext recovery
state may enter ordinary Dioxus state, URLs, clipboard, logs, diagnostics,
analytics, crash reports, or public profile metadata. Transient Rust, Swift,
and Kotlin buffers remain bounded, redacted, and zeroizing where supported.

## Why PRF wraps instead of derives

The WebAuthn credential is scoped to a relying party. The PRF extension exposes
one or two credential-scoped pseudorandom outputs, but the specification allows
an authenticator not to return output during credential creation. Support and
restore semantics require runtime detection and physical-device evidence.

Directly deriving wallet accounts from PRF would bind every address to one
credential, relying-party configuration, and provider lifecycle. A wrapped
random secret instead makes the passkey replaceable. It also keeps the later
addition of a second passkey, peer device, encrypted export, share recovery, or
threshold signing behind the same envelope boundary.

Secure Enclave and Android Keystore/KeyMint remain device protection, not
backup. A hardware-backed key may protect a local envelope copy and gate use,
but device loss must not destroy the wallet. Platform backup facilities may
transport only Oxid-encrypted, versioned data; they must not receive plaintext
or accidentally upload device-bound vault material.

## Deferred options

The attached research identified useful later options, but none belongs in the
first implementation:

- **OpenSigner** is MIT-licensed and self-hostable. Its current documentation
  describes device, hot, and cold Shamir shares with ephemeral full-key
  reconstruction for signing. That is share-based key management, not true
  threshold signing, and its documented Ethereum/Solana support does not prove
  Midnight/Jubjub compatibility.
- **Web3Auth MPC Core Kit** exposes an embedded-wallet MPC client. The full
  service's self-hosting, authentication, recovery, export, migration, metadata,
  liveness, and curve properties require independent evaluation.
- **Lit Protocol** exposes public repositories for a distributed programmable-
  key network. It brings network operators, policy, economics, liveness, and
  supported-curve assumptions that are unnecessary for the first wrapper.
- **Shamir recovery, guardian recovery, FROST/DKG, managed MPC, and
  contract-level authority rotation** each have a distinct threat model and
  require a separate issue-backed ADR before adoption.

No candidate is admitted merely because it uses “non-custodial,” “MPC,” or
“threshold.” Review must determine whether the complete key is reconstructed,
who operates required services, which parties can deny signing or recovery,
what metadata they observe, which algorithms they support, and how an owner
exits the system.

## Staged delivery

### Stage 0: specify and preserve compatibility

- Keep the versioned authenticated complete-wallet envelope and CSPRNG root.
- Specify the relying-party/origin model, encrypted blob location, schema,
  domain separation, factor identifiers, rollback counters, and size bounds.
- Keep mnemonic/raw-root import and encrypted export in a clearly labelled
  advanced path.
- Define provider migration as add-new-wrapper-before-remove-old-wrapper.

### Stage 1: physical-device capability spike

- Register and authenticate Level 2-compatible passkeys on current physical
  iOS and Android devices with Apple and Google credential providers.
- Detect and evaluate PRF for the exact credential without handling wallet
  material.
- Record support, sync, restore, export, cancellation, and provider-change
  behavior; fail closed when PRF is absent.

### Stage 2: one PRF-wrapped recovery envelope

- Wrap the random envelope key after fresh user verification.
- Store only authenticated ciphertext and bounded opaque metadata.
- Restore the same test wallet on a second or reset device through a
  synchronized credential.
- Keep the existing encrypted export as an independent fallback and expose the
  current recovery posture truthfully.

### Stage 3: rotation and hardening

- Add a new passkey/provider wrapper before removing the old one.
- Add rollback protection, recovery receipts, cancellation, rate limits, and
  safe local cleanup.
- Complete the adversarial and account-loss test matrix before considering the
  mnemonic hidden by default.

Advanced share recovery, threshold signing, guardian recovery, and
network-specific authority rotation remain separate future decisions.

## Consequences

- The first implementation is small enough to validate without deploying MPC,
  guardian, database, or always-on Oxid services.
- Onboarding can become mnemonic-free without weakening root entropy or hiding
  a custody operator.
- Apple and Google improve recovery UX but do not become blockchain identities
  or sources of wallet entropy.
- Existing wallet addresses survive passkey or provider rotation because the
  wrapped random secret is stable.
- A missing PRF capability blocks this recovery path; authentication success
  alone must not be presented as wallet recovery.
- This Proposed ADR approves neither implementation nor dependency.

## Required evidence before acceptance

- Physical iOS and Android matrix for passkey registration, authentication,
  PRF capability, synchronization, restore, export, provider change, and
  unsupported behavior.
- A versioned envelope threat model proving that ciphertext, a cloud account
  session, or a stolen locked device alone cannot recover spend authority.
- Create, reinstall, new-device, offline-use, provider-outage,
  cloud-account-loss, key-invalidation, cancellation, and rollback tests.
- A migration drill that adds a new wrapper before removing the old provider
  and retains the same wallet accounts.
- Exact Midnight derivation/signature evidence after recovery, including
  Jubjub-specific evidence where a role requires it.
- UX review proving that “seedless” is not presented as “secretless,” that
  provider dependence is disclosed, and that advanced export remains usable.

## Rejected alternatives

- **Derive a seed from an account, email, password, or OAuth value.** These
  values lack wallet-root entropy and create an account-takeover root of trust.
- **Derive the wallet root directly from passkey PRF.** This couples all wallet
  addresses to one credential lifecycle and prevents independent rotation.
- **Assume equal PRF output for equal cloud accounts.** PRF output is
  credential- and relying-party-scoped; an account is not a deterministic-seed
  protocol.
- **Treat WebAuthn Level 2 authentication as PRF support.** PRF is a Level 3
  extension and must be detected separately.
- **Use platform automatic backup as custody.** Silent backup provides neither
  explicit recovery evidence nor provider independence.
- **Treat Shamir reconstruction as threshold signing.** Secret splitting
  protects storage, but the complete key exists during signing.
- **Remove advanced export immediately.** Cross-platform PRF recovery needs
  physical-device evidence before it becomes the only survivable route.
- **Merge secret and network QR payloads.** Issue #359 keeps secret recovery
  envelopes separate from public connection manifests and protocol routing.

## Open questions

1. Which current iOS and Android APIs expose WebAuthn PRF to Oxid's native app,
   and through which Apple, Google, or third-party credential providers?
2. Does a synchronized credential preserve usable PRF behavior through device
   restore and cross-platform provider export?
3. What relying-party/origin topology supports native mobile and desktop
   recovery without an unnecessary permanent application-service dependency?
4. Where should authenticated ciphertext live for the first test: platform
   cloud storage, a blind blob service, user-selected storage, or peer transfer?
5. Which advanced encrypted export is the independent fallback for the MVP?
6. Are DIDs and VCs restored with wallet authority, independently backed up, or
   reissued under continuity proofs?

## References

- [BIP39: Mnemonic code for generating deterministic keys][bip39]
- [WebAuthn Level 2][webauthn2]
- [WebAuthn Level 3 PRF extension][webauthn-prf]
- [Apple Passkeys][apple-passkeys]
- [Google passkey supported environments][google-passkeys]
- [FIDO Credential Exchange Specifications][fido-exchange]
- [NIST SP 800-63B authenticator requirements][nist-800-63b]
- [Apple Secure Enclave key protection][secure-enclave]
- [Android Keystore/KeyMint][android-keystore]
- [OpenSigner source and architecture][opensigner]
- [Web3Auth MPC Core Kit source][web3auth-core]
- [Lit Protocol public repositories][lit]
- [NIST Threshold Cryptography project][nist-threshold]
- [RFC 9591: FROST][rfc9591]

[bip39]: https://github.com/bitcoin/bips/blob/master/bip-0039.mediawiki
[webauthn2]: https://www.w3.org/TR/webauthn-2/
[webauthn-prf]: https://www.w3.org/TR/webauthn-3/#prf-extension
[apple-passkeys]: https://developer.apple.com/passkeys/
[google-passkeys]: https://developers.google.com/identity/passkeys/supported-environments
[fido-exchange]: https://fidoalliance.org/specifications-credential-exchange-specifications/
[nist-800-63b]: https://pages.nist.gov/800-63-4/sp800-63b/authenticators/
[secure-enclave]: https://developer.apple.com/documentation/security/protecting-keys-with-the-secure-enclave
[android-keystore]: https://source.android.com/docs/security/features/keystore
[opensigner]: https://github.com/openfort-xyz/opensigner
[web3auth-core]: https://github.com/Web3Auth/mpc-core-kit
[lit]: https://github.com/LIT-Protocol
[nist-threshold]: https://csrc.nist.gov/projects/threshold-cryptography
[rfc9591]: https://www.rfc-editor.org/info/rfc9591/
