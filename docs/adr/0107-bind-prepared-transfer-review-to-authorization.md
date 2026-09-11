# ADR-0107: Bind prepared transfer review to authorization

- Status: Accepted
- Date: 2026-09-09
- Source: issue #108

## Decision

Only retained draft identifiers and an authorization challenge cross the wallet
application boundary for prepared-transfer authorization. `WalletTransferPreviewView`
derives its review title and summary from retained authoritative fields; callers
cannot supply authorization prose. The Midnight adapter domain-separates and
length-prefixes every rendered preview semantic field into the single-use
challenge. A successful authorization consumes that challenge and cannot be
replayed.

`oxid.headless.v1` retains its legacy confirmation object only as a bounded
adapter-level compatibility input. It is validated and discarded before calling
the application.

## Consequences

A rendered transfer review and the authorization it unlocks have one trusted
source. Serialized transactions, signatures, generic signing, submission
confirmation, MCP authority, and native user-presence mechanisms are unchanged.
