# Credential issuance activity

Credential issuance activity is an application-owned, privacy-safe read projection, not an OpenID4VCI event log.

| Application event | Activity status | Finality |
| --- | --- | --- |
| issuance admitted before `issue` | `pending` | `pending` |
| verified credential accepted by the local sink | `stored` | `final` |
| protocol definitively rejects issuance, or local verification rejects the credential | `failed` | `final` |
| protocol is unavailable, or local storage reports an uncertain write outcome | `outcome_unknown` | `unknown` |
| protocol discard succeeds after refusal | `refused` | `final` |
| future protocol cancellation evidence | `cancelled` | `final` |
| future protocol timeout evidence | `timed_out` | `unknown` |
| issuance future is dropped/interrupted after admission | `outcome_unknown` | `unknown` |

`cancelled` and `timed_out` are reserved for matching protocol evidence; current adapters do not emit either event. Navigating away from the UI creates neither event. Discarding a session after an uncertain result does not prove that the remote issuer did nothing, so the activity remains `outcome_unknown`. An unknown outcome may later be reconciled only by definitive protocol or sink evidence.

## Command and event behavior

| Input or race | Result |
| --- | --- |
| duplicate begin for one prepared issuance | reuse its application activity ID |
| duplicate or stale terminal update | keep the first final outcome |
| completion after the session has left `issuing` | reject the stale completion |
| explicit refusal before acceptance | discard the prepared protocol session, then record `refused` |
| refusal after an uncertain outcome | discard the local session; retain `outcome_unknown` activity |
| retry after a terminal protocol session | reject with invalid state; a new offer requires a new prepared session and activity ID |
| replacement offer | prepare a new session; no activity is created until acceptance or explicit refusal |
| UI cancellation or timeout without protocol evidence | create no cancellation or timeout activity |
| process restart or recovery | start with an empty activity projection; do not reconstruct events from documents or navigation |

Records have a bounded monotonic application activity ID which is unrelated to the protocol `issuance_id`. They are separated by wallet profile, retained only in process memory, deleted on restart, and never backed up. The oldest final record is evicted when the limit of 128 is reached; pending and uncertain records are retained. If there is no room, new issuance is refused before protocol execution. The projection does not retain or expose offers, protocol identifiers, credential bytes, claims, proofs, keys, or raw adapter errors.
