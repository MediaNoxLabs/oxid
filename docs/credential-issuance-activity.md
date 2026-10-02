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
| local discard failure before acceptance | retain `awaiting_consent`; create no issuance activity |
| local discard after a definitive failure | clear the local protocol session; keep `failed` in both read models |
| refusal after an uncertain outcome | discard the local session; retain `outcome_unknown` activity |
| duplicate refusal after successful local discard | return the retained session view without calling the protocol adapter again |
| retry after a terminal protocol session | reject with invalid state; a new offer requires a new prepared session and activity ID |
| replacement offer | prepare a new session; no activity is created until acceptance or explicit refusal |
| UI cancellation or timeout without protocol evidence | create no cancellation or timeout activity |
| process restart or recovery | start with an empty activity projection; do not reconstruct events from documents or navigation |

Records have a bounded monotonic application activity ID which is unrelated to the protocol `issuance_id`. They are separated by wallet profile, retained only in process memory, deleted on restart, and never backed up. Each profile retains at most 128 records independently: its oldest final or uncertain record is evicted when that profile needs room, while active pending records are retained. Activity in one profile cannot evict another profile's history. If a profile's 128 records are all pending, acceptance returns `unavailable` before protocol execution and keeps the prepared session awaiting consent. The user can retry that same session when capacity opens; no untracked issuance is started. An explicit `clear_profile` hook purges a profile's records after its sessions are discarded. The current app has no profile-deletion flow, so restart is the current production deletion event. The projection does not retain or expose offers, protocol identifiers, credential bytes, claims, proofs, keys, or raw adapter errors.

Completed protocol sessions are also bounded to the 128 most recently completed terminal entries per profile. Pending and `outcome_unknown` sessions are retained because they still need idempotent handling or reconciliation; an evicted terminal session is no longer queryable, so clients must create a new offer rather than retry it.

The displayed issuer is an endpoint from issuer metadata validated by the production OID4VCI adapter before it returns a prepared preview. It identifies the responding issuer endpoint, not an independent trust endorsement or a claim that a credential was issued. Activity ordering uses insertion order; wall-clock observation time is display metadata and may move backwards if the device clock changes. A refusal is still sent to the protocol when the Activity buffer is full, but in that case no refusal record is added.
