# ADR-0112: Adopt private Docker Sandboxes as an opt-in Pi deploy target

- Status: Accepted
- Date: 2026-09-26
- Source: issue #763 and a supervised macOS/Apple-silicon proof of concept
- Related: ADR-0022, ADR-0096, ADR-0107, and issues #198/#227/#655/#763
- Implementation state: exact package pin, fail-closed launcher, and deploy-target
  boundary delivered; full sandbox execution remains a follow-up

## Context

Independent Pi workers currently share host-global resources: the Docker
daemon, Compose project names, published ports, caches, Tailnet routes,
simulators, and device bridges. A linked Git worktree isolates source state but
does not isolate any of those resources. Random ports reduce one collision and
lock files serialize one resource, but neither protects the host Docker daemon
or gives a worker a disposable runtime.

[`@stixxert/pi-docker-sandbox`](https://pi.dev/packages/@stixxert/pi-docker-sandbox)
provides two materially different modes. Its default extension keeps Pi on the
host and adds a private Docker Sandbox microVM and Docker daemon as a deploy
target. Its optional `sandbox/` backend overrides Pi's normal filesystem and
shell tools so they execute in that microVM. These modes have different
toolchain, Git, credential, cache, and evidence requirements and must not be
treated as interchangeable.

Oxid needs a useful incremental adoption, not a new mandatory factory layer.
Documentation, policy, macOS/iOS builds, physical devices, Tailnet routes,
signed commits, and GitHub mutation still belong on the host. Simulator-free
headless builds and disposable Docker environments are candidates for an
isolated worker only when their complete toolchain and evidence boundary are
known.

## Decision

Pin `@stixxert/pi-docker-sandbox@1.1.6` in the content-addressed project Pi
closure, but suppress its extension in ordinary sessions. An issue worker opts
in through `scripts/factory/pi-docker-sandbox.sh`. The launcher:

- retains the repository's audited host Pi, model authentication, sessions,
  Git/GPG configuration, and tracked source-editing tools;
- explicitly loads only the package's default `index.ts` deploy-target
  extension, never the full `sandbox/` execution backend;
- requires an authenticated `sbx` CLI and fails before model dispatch when it
  is unavailable;
- unsets the package's unsandboxed fallback, broad environment pass-through,
  environment allowlist, and persistent sandbox name;
- mounts the project read-only into the deploy target;
- creates a unique session sandbox with 4 CPUs and 8 GiB by default and removes
  it at session shutdown; and
- uses the ordinary `bootstrap.sh --pi` audit and startup smoke before Pi runs.

The package remains optional because routine work should not pay the prompt,
startup, VM, image, or disk cost of Docker tools. It is appropriate when the
target plan contains a disposable Docker build, Compose stack, headless service
integration, or service port that must not touch the host daemon. It is not an
authorization to expose credentials, the host Docker socket, a physical
device, a simulator, or a Tailnet route to the sandbox.

### Routing matrix

| Work or resource | Execution lane | Reason |
| --- | --- | --- |
| Docs, ADRs, repository contracts, Git/GPG, GitHub issue/PR operations | Host Pi | No runtime isolation benefit; host custody remains authoritative |
| Docker image builds, disposable Compose stacks, headless service integration | Host Pi + private deploy target | Source edits stay host-audited while Docker state and ports are session-private |
| Rust/Nix/headless compilation inside the microVM | Deferred execution backend | Requires a reviewed Nix-enabled template, cache policy, and complete Git checkout |
| macOS and iOS builds/simulators | Host-exclusive lease | Apple toolchains and simulator services are host resources |
| Physical Android/iOS devices | Host-exclusive lease | Device bridges, user presence, and native custody cannot be virtualized truthfully |
| Tailnet routes and real-device demos | Host-exclusive lease | One machine-wide route/port ownership domain; sandbox NAT would change the evidence |
| Shared standalone stack intentionally reused by people | Named host lease | Persistence is deliberate and must not be confused with a disposable worker sandbox |

Docker isolation complements rather than replaces resource leases. A worker
must still acquire an atomic host-global lease before using a simulator,
physical device, Tailnet route, fixed public host port, shared cache mutation,
or persistent named stack. Randomized ports are a convenience within an owned
lane, not the ownership protocol.

### Credentials, source, and caches

Model credentials remain in the host Pi process. GitHub tokens, signing keys,
native custody, Tailnet state, Docker contexts, and arbitrary host environment
variables are not copied or forwarded. A future sandbox workload that needs a
credential requires an issue-backed, least-privilege secret-injection design;
ordinary host environment pass-through remains forbidden.

The deploy-target mode reads the current worktree through the package's bounded
path mapping and read-only mount. A future full execution backend must use a
sandbox-local clone or a complete bounded mount of both worktree and Git common
metadata. Mounting only a linked worktree is invalid because its `.git` file
points to metadata outside that mount.

Do not mount the host Nix store read-write. A future execution image may use a
pinned Docker Sandbox template plus an immutable binary cache and sandbox-local
Nix/Cargo stores. Cache keys must include the lockfile/toolchain and target;
cache corruption or absence must degrade to a rebuild, never to unreviewed host
state.

### Lifecycle and observability

Default package-generated names (`pi-sbx-<pid>-<random>`) provide session
uniqueness. The launcher rejects persistent naming by unsetting
`DOCKER_SANDBOX`. Session teardown is `remove`; package watchdog and stale
sandbox collection cover abnormal termination. Operators can audit with
`docker_verify`, `docker_resources`, and `sbx ls`. The factory records at least
startup time, image/build/gate duration, model/tool/token counters, peak
resource use when available, bytes retained, cleanup outcome, and whether any
exclusive host lease was used.

## Supervised proof of concept

The 2026-09-26 probe installed `sbx` v0.45.1, used the reviewed balanced network
policy, and created exactly one named 2-CPU/4-GiB shell sandbox. The sandbox was
Linux/aarch64 and isolated from the host Docker daemon. It then failed closed
before Pi or a gate ran:

- the generic shell image had Node but no Pi, Nix, or Cargo;
- the linked worktree mount omitted Git-common worktree metadata, so even
  `git status` could not establish repository identity; and
- therefore it could not produce truthful repository startup or local-gate
  evidence.

The exact sandbox was removed and `sbx ls` was empty. The Pi worker consumed
10m58s, 32 turns, 43 tool calls, 127,101 input and 7,782 output tokens
($0.7339412 reported model cost). That is useful discovery evidence but poor
delivery throughput. It justifies the small deploy-target adoption and a
separate bounded execution-template task rather than an open-ended attempt to
repair an unknown image inside a feature run.

After the bounded deploy-target adoption, an offline/no-session RPC startup
exposed `/docker` from the exact `1.1.6` package path. A package-owned canary
then invoked only `docker_status` and `docker_verify`; every isolation check
passed, no container or port was created, session teardown removed the
microVM, and final `sbx ls` was empty. This proves the admitted mode without
claiming that the deferred execution backend can build Oxid.

## Follow-up execution-backend gate

The package's `sandbox/` backend may be admitted only when one focused issue
demonstrates all of the following in two concurrent issue worktrees:

1. a pinned, reviewed Nix-enabled template or equivalent immutable bootstrap;
2. sandbox-local Git identity with no foreign host metadata or credential copy;
3. exact repository Pi startup smoke and one simulator-free local target gate;
4. private ports/Compose state and no host Docker containers or networks;
5. bounded cache ownership and reproducible cold/warm measurements;
6. automatic cleanup after normal exit, interruption, and failed startup; and
7. retained run metrics proving a material benefit over sequential host lanes.

Failure of any condition keeps the execution backend disabled. It does not
roll back the safe deploy-target mode.

## Consequences

- Parallel Pi sessions can own separate Docker daemons without randomized
  Compose names or access to the host socket.
- Ordinary Pi remains unchanged and cheap; sandbox use is visible and
  intentional in the launch command.
- Host credentials and native/mobile resources stay outside the microVM.
- The first adoption does not yet isolate Rust/Nix compilation or guarantee a
  faster cold start.
- Full tool execution becomes a measurable follow-up instead of hidden scope
  inside product delivery.

## Alternatives rejected

- **Give every worker the host Docker socket:** fastest initially, but a typo or
  broad cleanup can stop unrelated stacks and concurrent workers cannot prove
  ownership.
- **Randomize ports and Compose project names only:** useful defense in depth,
  but still shares the daemon, volumes, networks, disk, and cleanup authority.
- **Use lock files alone:** correct for scarce host resources, but serializes
  Docker work and does not contain it; retained as the host-resource mechanism.
- **Run the complete Pi process in a general Docker container:** couples model
  auth, sessions, Git signing, package state, and workspace custody to an image
  while losing native tool access.
- **Enable the package's execution backend immediately:** the live probe proved
  the generic image lacks Oxid's pinned toolchain and complete Git topology.
- **Make Docker Sandboxes mandatory:** adds cost and a macOS/Linux hypervisor
  dependency to changes that need neither Docker nor isolation.

## Research sources

- [Pi package catalog: `@stixxert/pi-docker-sandbox`](https://pi.dev/packages/@stixxert/pi-docker-sandbox)
- [Package source and boundary documentation](https://github.com/stixxert/pi-docker-sandbox)
- [Docker Sandboxes documentation](https://docs.docker.com/ai/sandboxes/)
