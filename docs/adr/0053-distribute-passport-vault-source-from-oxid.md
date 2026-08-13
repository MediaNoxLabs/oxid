<!-- SPDX-License-Identifier: Apache-2.0 -->

# ADR-0053: Distribute the reviewed Passport Vault source from Oxid

- Status: Accepted
- Date: 2026-08-14
- Blueprint: §§3–8, 12–13, 16–18, 21
- Upstream source: private `midnightntwrk/midnight-identity-solution-examples` commit `e4a92a6be2cc6dc34f68261f10c19c9312043807`, `packages/contracts/vault/src/passport-vault.compact`
- Distributed source: `contracts/passport-vault/passport-vault.compact`, 23,776 bytes, SHA-256 `2ebc5b34dd440bc9a9736408f29f5003e7a78f26a564b392be2af36de69102f4`
- License: Apache-2.0, as declared by the upstream repository and `@input-output-hk/passport-vault-contract` package
- Related: ADR-0004, ADR-0006, ADR-0020, ADR-0022, ADR-0052, issue #31
- Supersedes: only ADR-0052's decision to fetch the Passport Vault source as a flake input from its upstream repository
- Implementation state: byte-identical source distribution, public Nix composition, digest assertion, and license sidecar are implemented

## Context

ADR-0052 authenticated the reviewed Passport Vault contract by pinning its
upstream Git revision as a non-flake Nix input. That works for developers with
access to the companion repository, but the repository is private. An
unauthenticated clean GitHub Actions runner receives HTTP 404 before it can
enter the development shell. Documentation, quality, and normal CI therefore
cannot validate a public Oxid commit without a private organization token.

Public-repository validation must work for forks and pull requests without
secrets. The contract is the preferred source form, is Apache-2.0 licensed, is
small enough to review, and is required to reproduce the generated schema and
proof artifacts. Generated JavaScript, IR, parameters, and proving keys remain
large derivable outputs and do not have the same reason to enter Git.

## Decision

Oxid distributes a byte-identical copy of the reviewed Compact contract at
`contracts/passport-vault/passport-vault.compact`. The adjacent `.license`
sidecar records Apache-2.0 without changing the authenticated source bytes.
The provenance record retains the private upstream repository, exact revision,
path, byte count, and SHA-256 digest.

The Nix derivation consumes the Oxid path and fails before compilation unless
its digest is exactly
`2ebc5b34dd440bc9a9736408f29f5003e7a78f26a564b392be2af36de69102f4`.
It continues to fetch the public VC and Compact toolchain repositories at
immutable revisions, compiles all five circuits, and emits the same generated
artifact manifest. The private upstream repository is no longer a flake input
or a CI credential requirement.

Changing the distributed contract is a coordinated contract-version review.
It requires a new upstream or Oxid-owned provenance baseline, updated digest,
regenerated layout fixtures and artifacts, circuit-size review, and a later
ADR. It is not an ordinary source edit.

## Rejected alternatives

- Giving public CI a token for the private companion repository would make
  fork validation secret-dependent and unnecessarily broaden credential use.
- Relying on a developer's Nix Git cache would make the build non-portable and
  hide the missing source from clean machines.
- Committing generated clients, IR, parameters, or proving keys would add large
  derivable content and weaken the single-source review boundary.
- Pointing at an assumed public mirror is not possible because no authoritative
  public mirror exists at this decision date.

## Consequences

- Clean public CI and forks can evaluate the shell and reproduce every vault
  artifact without access to a private organization repository.
- Oxid now owns distribution and review of one byte-identical third-party
  Compact source file while retaining exact upstream provenance and license.
- The source digest remains the compatibility boundary used by ADR-0052's
  native decoder and deterministic generated-client fixture.
- Generated content remains outside Git and inside the authenticated Nix
  closure.

## Validation

- `sha256sum contracts/passport-vault/passport-vault.compact`
- `nix build .#passport-vault-compact-artifacts --print-build-logs`
- `nix flake check --print-build-logs`
- an unauthenticated GitHub Actions checkout can enter `nix develop`
