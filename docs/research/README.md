# Market and ecosystem research

Competitive and ecosystem analysis used to steer the Oxid roadmap. Each study
records what a comparable product does, what it does better than Oxid, and
what that implies for our sequencing — with sources, so a claim can be
re-checked when the other project moves.

| Study | Subject | Date | Headline |
| --- | --- | --- | --- |
| [moth-wallet.md](moth-wallet.md) | `shieldedtech/moth-wallet` — Shielded Technologies' Midnight reference wallet | 2026-08-20 | Ships the DApp connector Oxid has no code and no ADR for; has no SSI layer at all |
| [web-bridge-architecture.md](web-bridge-architecture.md) | Chrome-extension-to-phone bridge vs a wasm web target — transports, precedent, threat model | 2026-08-20 | Custody thesis holds; "no infrastructure" does not; USB is dead on iOS; ship the QR channel first |

## How to use these

1. **Findings are dated and sourced.** Every factual claim carries a URL,
   commit, or command output. A study is a snapshot, not a standing truth —
   re-verify before acting on a months-old comparison.
2. **Be generous to the subject.** The purpose is to learn, not to win. A
   study that finds nothing worth copying has almost certainly been done
   badly.
3. **Separate "they are ahead" from "we should copy".** Some gaps are
   deliberate consequences of Oxid's architecture (fail-closed production
   composition, platform-backed custody). A study should say which gaps are
   choices and which are debts.
4. **Every actionable finding becomes an issue.** The study links to it, so
   research does not accumulate as unexecuted reading.

## Candidates for future studies

- **Lace** (Cardano, and its Midnight support) — the closest thing to an
  incumbent in both ecosystems Oxid targets.
- **EUDI reference wallet** — the regulatory reference for the identity half
  of the product; already informing `docs/design/`.
- **Identity wallets with credential UX at scale** — Microsoft Authenticator
  verified IDs, Apple/Google Wallet IDs, Lissi, Talao.
- **Agent-facing wallets** — Coinbase Agentic Wallet, Phantom's MCP surface,
  as ADR-0099's tool surface grows (issue #70).
