# System diagrams

Rendered from text (mermaid) so they stay reviewable in pull requests and
never rot as binary images. Each diagram names the crates it depicts —
when a crate moves, the diagram fails review, not reality.

## Context: Oxid in its ecosystem

```mermaid
flowchart LR
    subgraph People
      H[Holder<br/>mobile user]
      D[Developer / Agent<br/>headless NDJSON]
    end
    subgraph Oxid[Oxid wallet]
      APP[Dioxus app / oxid-headless]
    end
    subgraph Midnight[Midnight network]
      NODE[Node<br/>finalized blocks]
      IDX[Indexer<br/>GraphQL/WS]
      PROOF[Proof params<br/>Compact artifacts]
    end
    subgraph SSI[Identity ecosystem]
      ISS[Issuers<br/>OpenID4VCI]
      VER[Verifiers<br/>OpenID4VP / SIOPv2]
      RES[DID resolver]
    end
    H --> APP
    D --> APP
    APP -->|submit / replay| NODE
    APP -->|watch, unproven| IDX
    APP -->|prove locally| PROOF
    ISS -->|credentials| APP
    APP -->|proofs, never raw claims| VER
    APP --> RES
```

## Containers: the hexagonal workspace (37 crates)

```mermaid
flowchart TB
    subgraph Incoming
      UI[ui-dioxus]
      HL[oxid-headless]
    end
    subgraph Application["application layer (zero external deps)"]
      WA[wallet/application]
      IA[identity/application]
      CA[credential/application]
      PA[presentation/application]
      PRA[protocol/application]
      VA[passport-vault/application]
      DA[diagnostics/application]
    end
    subgraph Domain["domain layer (zero external deps)"]
      WD[wallet] --- ID[identity] --- CD[credential]
      PD[presentation] --- PRD[protocol] --- VD[passport-vault]
    end
    subgraph Adapters["outgoing adapters (external SDKs live here)"]
      MID[midnight<br/>node·indexer·proving]
      DIDM[did-midnight]
      VCM[vc-midnight<br/>Compact VC + ZK]
      OIDC[openid4vci · openid4vp · siopv2]
      CUST[custody-software · storage-mobile]
      STOR[storage-* · backup-*]
    end
    COMP[composition<br/>fail-closed root]
    UI --> Application
    HL --> Application
    Application --> Domain
    COMP --> Adapters
    Adapters -->|implement ports of| Application
```

Rules enforced by `scripts/check-architecture.sh` (default-deny): domain and
application crates take **no external dependencies**; adapters convert
external types at the boundary; only `composition` joins adapters to ports.

## Sequence: unshielded transfer (persist-before-broadcast)

```mermaid
sequenceDiagram
    actor U as User
    participant App as wallet/application
    participant M as adapters/midnight
    participant C as custody (opaque refs)
    participant N as Midnight node
    U->>App: prepare (recipient, amount)
    App->>M: plan inputs (UTXO selection, change)
    M-->>U: public preview
    U->>App: authorize (exact reviewed transfer)
    App->>C: sign via key reference
    Note over M: draft retained adapter-private (1 h TTL)
    U->>App: submit
    M->>M: DUST proof (local, k=13)
    M->>M: journal attempt BEFORE broadcast
    M->>N: submit extrinsic
    alt ambiguous outcome / worker death
      Note over M: stays Submitting — no blind retry,<br/>reconcile against finalized history only
    end
    N-->>App: finalized outcome → Confirmed
```

## Sequence: credential presentation with a real ZK proof

```mermaid
sequenceDiagram
    actor U as Holder
    participant P as presentation/application
    participant V as adapters/openid4vp
    participant VC as adapters/vc-midnight
    participant K as custody (Jubjub)
    V->>P: parsed request (verifier, DCQL)
    P-->>U: consent: WHO / WHAT / FROM / WHY
    U->>P: accept (explicit credential choice, ADR-0082)
    P->>VC: re-verify stored credential + proof + openings
    P->>K: reauthorize current managed method (ADR-0048)
    K-->>VC: holder Proof via challenge callback (ADR-0049)
    VC->>VC: Compact circuit prove k=18 + independent verify (ADR-0050)
    Note over VC: fail-closed: without authenticated artifacts →<br/>proof_unavailable, never a simulated boolean
    VC-->>V: vp_token (public MZP1 envelope only)
```

## Sequence: Passport Vault call (canonical replay required)

```mermaid
sequenceDiagram
    actor U as User
    participant VA as passport-vault/application
    participant M as adapters/midnight
    participant N as node (finalized)
    U->>VA: prepare operation
    VA->>M: require canonical_finalized_replay
    M->>N: replay every finalized block from deployment
    N-->>M: verified contract state
    M-->>U: preview (authenticated, labeled)
    U->>VA: authorize (exact intent, e.g. AUTHORIZE_PASSPORT_VAULT_CALL)
    VA->>M: compose via pinned generated composer
    M->>M: fund from NIGHT UTXOs · prove · journal
    M->>N: submit → finalized outcome authority
    Note over M: indexer data is never call authority<br/>(indexer_supplied_not_proven)
```
