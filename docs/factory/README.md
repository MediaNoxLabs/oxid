# AI Software Factory

The factory is Oxid's formalized agent-driven delivery system, proposed in
[issue #35](https://github.com/MediaNoxLabs/oxid/issues/35). The repository
itself is the coordination plane: issues are the backlog, pull requests are
work units, labels carry finite-state-machine state, checks are gates, and a
human holds merge authority.

| Document | Contents |
| --- | --- |
| [charter.md](charter.md) | Roles, responsibilities, and authority boundaries. |
| [fsm.md](fsm.md) | The work-item finite state machine: states, transitions, gate conditions, failure edges. |
| [claim-protocol.md](claim-protocol.md) | Decentralized claim/lease protocol so agents on different machines never double-work an item. |
| [metrics.md](metrics.md) | The measurements the factory watches and the current baselines. |

Tooling lives in [`.pi/extensions/factory.ts`](../../.pi/extensions/factory.ts)
(a [pi](https://pi.dev) repo-local extension) so any engineer or agent with the
repository checkout and a `gh` login can participate — from any machine, with
any LLM provider.

## Design constraints

1. **Zero behavior change until opted in.** The factory formalizes around the
   existing development flow; a work item enters the factory only when it
   carries a `factory:*` label.
2. **No coordination infrastructure.** GitHub is the lock service, the queue,
   and the audit log. Any agent that can run `gh` can participate.
3. **Provider-agnostic.** Roles reference capabilities, never a specific LLM.
   Model selection is configuration (`.pi/settings.json`, `.devloops`
   persona `defaultModel`), not process.
4. **Humans merge.** Agents deliver evidence; `.devloops` `humanMergeOnly`
   remains binding.
