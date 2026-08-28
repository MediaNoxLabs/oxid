<!-- SPDX-License-Identifier: Apache-2.0 -->

# Factory worker topology

Oxid supports several Pi sessions on one developer host, workers on separate
cloud hosts, and independently operated engineer checkouts. Parallel delivery
is safe only when every mutating parent session owns one issue and one isolated
worktree. GitHub issues, leases, branches, pull requests, and exact-head checks
are the shared coordination plane; a local process or filesystem is not.

## Concurrency scopes

| Scope | Default bound | Meaning |
| --- | --- | --- |
| Repository | Multiple issue-backed candidates; one guarded merge at a time | Development and PR CI may overlap. The guarded claim lease in #197 will prevent two hosts from taking the same issue. |
| Git common checkout on one host | Two active managed delivery worktrees | This is a disk and local-compute admission bound, not a repository-wide queue. A second clone has its own bound and cache accounting. |
| Parent Pi session | One remotely driven candidate | `.devloops` `queue.maxParallel: 1` limits one conductor. It does not prohibit another parent session working another issue. |
| Issue worktree | One mutating parent session | Never attach two writers to one worktree, branch, target directory, or session file. Extra sessions may inspect through read-only tools. |
| Sub-agent run | Two concurrent children, eight spawns | The user-level policy is installed independently on every host and applies inside that Pi process. |

Branch names are globally unique issue identities: `<type>/issue-<number>`. Two
workers must not create different implementations for the same issue unless a
human explicitly declares one an experiment. A worker may inspect another
candidate, but it must not commit, push, relabel, or post gate evidence for that
candidate without taking over its recorded lease.

## Multiple local sessions

Configure a host once, then create or reuse one canonical worktree per issue:

```bash
cd /path/to/oxid
./bootstrap.sh --configure-pi
./bootstrap.sh --check
./bootstrap.sh --audit-pi

node scripts/loop/ensure-worktree.mjs \
  --repo-root "$PWD" --issue 201 --branch feat/issue-201
node scripts/loop/ensure-worktree.mjs \
  --repo-root "$PWD" --issue 202 --branch fix/issue-202
```

Start one `./bootstrap.sh --pi` process from each returned worktree. The default
admission limit is two active managed delivery worktrees per Git common
checkout. Read-only analysis can run in another process with Pi's restricted
tool set, but it does not become another mutation lane:

```bash
./bootstrap.sh --pi --tools read,grep,find,ls
```

Do not share `target/`, `.direnv/`, Pi session files, or a live worktree between
parents. Linked worktrees reuse the common checkout's exact Pi package closure
and bounded `sccache`; their mutable Rust targets remain isolated.

## Another engineer or provider

Every operator starts from a normal clone and installs the host-local policy:

```bash
git clone https://github.com/MediaNoxLabs/oxid.git
cd oxid
git fetch origin integration
git switch --detach origin/integration
gh auth status
./bootstrap.sh --configure-pi
./bootstrap.sh --check
./bootstrap.sh --audit-pi
./bootstrap.sh --pi
```

Use Pi's interactive `/login` on that host when the selected provider requires
subscription or stored API-key authentication. Configure the operator's Git
identity and approved signing key before creating commits.

The tracked default is `openai-codex/gpt-5.6-terra:medium`, not a provider
lock. Pi accepts a deliberate session override, for example
`./bootstrap.sh --pi --provider openai --model <model>`. The alternate provider
must satisfy the same issue, evidence, commit-signing, and gate contract.

Each engineer supplies their own GitHub and model-provider authentication.
`./bootstrap.sh --configure-pi` preserves unrelated Pi settings and never reads
or writes `auth.json`. Credentials, personal trust decisions, signing keys, and
session transcripts are host state and must not be committed or copied into a
shared worktree.

## Cloud workers

A cloud worker is possible when it has an isolated checkout, Nix, Git/GitHub
credentials with only the required repository authority, a signing identity,
and supported model-provider authentication. For an unattended Pi invocation,
pass `--approve` to trust the checked-out project for that run and inject an
API-key provider through its documented environment variable. An interactive
subscription login stores OAuth material in `~/.pi/agent/auth.json`; treat that
file as a credential secret and mount it only on an operator-approved host.
With the provider credential already injected by the runner, a bounded
non-interactive entry point has this shape:

```bash
./bootstrap.sh --pi --approve --provider openai --model <model> \
  --print "Deliver the human-assigned issue using the repository factory contract."
```

Persist only immutable or content-addressed infrastructure across ephemeral
workers: the Nix store/cache, Cargo registry, and bounded compiler cache. Never
mount one mutable Git worktree or Pi session directory into two workers. Store
the final record with `metrics.mjs write --output-dir` on an approved private
durable volume. Delete the ephemeral checkout only after its branch, exact
head, PR, private metric record, and bounded closeout have been preserved.

Cloud workers are not autonomous claimants until the atomic lease work in
[#197](https://github.com/MediaNoxLabs/oxid/issues/197) lands. Until then, a
human assigns a distinct issue to each host and the disabled `/factory claim`
command continues to fail closed. Portable two-worker conformance is tracked
by [#199](https://github.com/MediaNoxLabs/oxid/issues/199).

## Cross-host supervision

Raw metrics remain in each Git common directory or an approved durable private
directory. Every final PR head also gets the bounded, redacted closeout comment
defined in [metrics.md](metrics.md); those comments are the current cross-host
supervisor feed. A central database is unnecessary until retained records
demonstrate a query or coordination need that GitHub comments cannot satisfy.
Never place prompts, transcripts, credentials, raw model output, or billing
data in a PR comment or shared metric service.
