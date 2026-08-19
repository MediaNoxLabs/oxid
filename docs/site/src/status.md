# Delivery status

Oxid ships capabilities in reviewed slices, and every capability carries an
explicit mode label. This page is the reader's map; the authoritative,
always-current source is the delivery-state column of the
[ADR index](https://github.com/MediaNoxLabs/oxid/blob/develop/docs/adr/README.md)
and the repository [issue backlog](https://github.com/MediaNoxLabs/oxid/issues).

## The three modes

| Mode | Meaning |
| --- | --- |
| **Production composition** | What a plain build wires. Fails closed for every capability that has not passed review — today that means profile management plus native mobile custody initialization, and nothing else. |
| **Standalone development** | Explicit opt-in composition for simulators and headless runs: process-local development custody, deterministic simulations, and the real standalone SSI flows. |
| **Live standalone** | Standalone composition pointed, via explicit headless environment configuration or the compile-time mobile tailnet profile, at real Midnight infrastructure (indexer, node, proof parameters). |

## Wallet (Midnight) capabilities

| Capability | State |
| --- | --- |
| Profiles (create/list/select/restore) | Functional, persisted public metadata |
| Protected custody (Keychain/Keystore vault) | Native mobile sealing functional; production flows gated |
| Accounts, addresses, receive QR | Standalone functional; live subscription via explicit config |
| Unshielded transfers (prepare → authorize → prove → submit → reconcile) | Standalone functional incl. live node submission path; persist-before-broadcast journal |
| DUST + shielded sync | Resumable, checkpointed, cancellable; simulated and live-configured variants |
| Shielded transfers (fresh sync → prepare → authorize → prove → submit) | Standalone functional with adapter-private Zswap notes/witnesses; production mobile gated |
| Passport Vault contract calls | Typed lifecycle with canonical finalized replay; claim path gated behind consent + funding + settlement review |
| Portable + complete-wallet encrypted backup/recovery | Functional with hardened v3 KDF policy and native document pickers |
| Secret-safe runtime diagnostics | Bounded process-local closed codes in headless and Dioxus; telemetry, payloads, upload, and persistence are off |
| Developer/demo UI profiles | Separate compile-time standalone profiles; normal release marker scan proves both are excluded |
| Physical Android standalone routes | Compile-time development-only MagicDNS/TLS profile and repository-owned loopback stack harness; device/live-flow evidence remains separate |

## Identity (SSI) capabilities

| Capability | State |
| --- | --- |
| did:midnight resolution + inventory | Functional (fixture + native resolver via explicit config) |
| DID lifecycle (create/update/sign/deactivate) | Standalone functional for undeployed DIDs; live Compact writes deferred |
| Credential inventory + structured verification | Functional; seven-stage verification reports, encrypted storage |
| OpenID4VCI pre-authorized issuance | Standalone functional with exact Compact credential bundles |
| SIOPv2 self-issued authentication | Standalone functional (draft-13 subset) |
| Digital Passport disclosure planning + local reveal | Functional; explicitly not a verifier presentation |
| OpenID4VP presentation with real ZK proof | Functional in native headless composition and the explicit native-custody mobile conformance build; normal/production mobile remains gated pending physical-device budgets |
| Native QR and identity links | Typed iOS/Android capture plus strict shared routing; physical Android success/cancel/timeout and warm/cold custom schemes proven on Samsung/API 36, while physical iOS and verified HTTPS associations remain pending |

## The road to a shippable MVP

The load-bearing open pillars, each tracked publicly: mobile Compact proving
budgets ([#30](https://github.com/MediaNoxLabs/oxid/issues/30)), the live
Passport Vault adapter ([#31](https://github.com/MediaNoxLabs/oxid/issues/31)),
physical-device ingress evidence
([#32](https://github.com/MediaNoxLabs/oxid/issues/32)), physical-device
recovery interruption/resource evidence
([#33](https://github.com/MediaNoxLabs/oxid/issues/33)), standalone
issuer time policy ([#34](https://github.com/MediaNoxLabs/oxid/issues/34)),
plus live protocol transport and production issuer trust policy.

An independent architecture and quality review of the whole codebase (11
dimensions, adversarially verified findings) is published as
[Discussion #37](https://github.com/MediaNoxLabs/oxid/discussions/37).
