<!-- SPDX-License-Identifier: Apache-2.0 -->

# Opt-in Pi.dev observability

Oxid records local Pi.dev runtime metadata so the supervisor can compare lanes,
profiles, tool usage, duration, and token buckets without collecting prompts or
responses. Observability is **off by default**. The project pins
`@grafana/agento11y-pi@0.25.0`, but suppresses its extension during normal Pi
startup. Only the explicit launcher below instruments a new session.

## Start an observed session

Start from the Nix development shell after `./bootstrap.sh --check`:

```bash
node scripts/factory/pi-observability.mjs on \
  --lane supervisor \
  --profile production-ready \
  --work-type feat \
  --delivery-target milestone \
  --
```

The wrapper starts the local receiver and invokes the supported
`agento11y pi --no-local --tag key=value ...` launcher. It explicitly loads the
exact extension from Oxid's content-addressed package closure because normal
project startup suppresses auto-discovery. Arguments after `--` go to Pi. The
wrapper forces metadata-only capture, disables cloud forwarding, and does not
enable guard/content inspection.

Agento11y `v0.48.0` currently forces `contentCapture: full` when its plugin sees
`AGENTO11Y_LOCAL=true`. Oxid therefore starts the local receiver separately and
uses the normal metadata-only exporter with its endpoint, authentication
placeholders, and OTLP endpoint all bound to `127.0.0.1:8765`. A contract smoke
must confirm that the stored generation has token/timing metadata while
`messages`, `input`, and `output` are absent. Automatic user/repository/branch
tags are also disabled for observed Oxid sessions; only the bounded tags below
plus the plugin's unavoidable generation attributes are retained.

Launch a deliberately unobserved session with plain Pi semantics:

```bash
node scripts/factory/pi-observability.mjs off --
```

`off` fails closed if a personal Pi configuration independently enables the
Agento11y package; otherwise a plain Pi process cannot accidentally inherit the
project extension. Loading or unloading applies to a newly launched process;
it never mutates a running Pi session.

Inspect or stop the receiver:

```bash
node scripts/factory/pi-observability.mjs status
node scripts/factory/pi-observability.mjs stop
```

## Bounded tag vocabulary

Every observed run has exactly these project tags:

| Key | Allowed values |
| --- | --- |
| `project` | `oxid` |
| `factory` | `pi-dev` |
| `environment` | `local` |
| `lane` | `supervisor`, `host-headless`, `host-mobile`, `docker` |
| `profile` | `prototype`, `production-ready`, `research` |
| `work_type` | `feat`, `fix`, `docs`, `test`, `refactor`, `chore` |
| `delivery_target` | `develop`, `milestone`, `none` |

Do not add issue numbers, PR numbers, branch names, paths, prompts, user IDs, or
secrets as custom metric tags. Agento11y may emit its documented built-in
`git.branch`, `cwd`, and `pi.call_kind` attributes; analysis and dashboards
should group primarily by the bounded project vocabulary. Agento11y's local
`v0.48.0` summary API does not expose arbitrary tag filters yet. The tags are
still emitted for a future supported OTLP/Grafana data path, while today's
local dashboard uses its session, model, branch, workspace, and tool summaries.

## Local Grafana

The current local adapter is intentionally narrow: Agento11y `v0.48.0` exposes
versioned loopback JSON endpoints under `/api/v1/metrics/*`; Grafana Infinity
`4.1.0` reads only those endpoints. This is a local compatibility adapter, not
a public network interface or a replacement for a supported OTLP export.

Install or refresh the repository-owned datasource and dashboard:

```bash
node scripts/factory/grafana-pi-dashboard.mjs install
node scripts/factory/grafana-pi-dashboard.mjs status
```

Open <http://127.0.0.1:3000/d/oxid-pi-factory>. The dashboard shows session and
error counts, token buckets, recent sessions, tool execution, and token trends.
An empty dashboard means no observed Pi session has completed yet; it is not a
receiver failure.

The installer supports the Homebrew Grafana layout, pins the Infinity plugin,
allows only `http://127.0.0.1:8765`, and provisions files carrying the Oxid
ownership marker. Grafana credentials are neither read nor stored.

Rollback only the Oxid dashboard and datasource while retaining the shared
Infinity plugin:

```bash
node scripts/factory/grafana-pi-dashboard.mjs remove
```

Then stop capture with `pi-observability.mjs stop`. If an Agento11y upgrade
changes `/api/v1/metrics/*`, the dashboard must fail visibly and this adapter
must be updated before the pinned version changes.
