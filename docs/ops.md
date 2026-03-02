---
status: active
milestone: kernel
spec: null
code: docker-compose.yml, docker/, src/telemetry/
---

# Operations

*Observability, backups, alerting, and operational runbook for the animus appliance.*

## Instance Model

Every animus instance is an OS user account. On Linux: `useradd cookie`. On macOS: `sysadminctl -addUser cookie`. The OS provides isolation — file permissions, process ownership, resource limits. No custom instance management.

Each user account runs its own appliance (Docker Compose stack). Paths follow the XDG Base Directory Specification. The animus binary reads XDG paths for the current user — no `ANIMUS_HOME`, no `ANIMUS_INSTANCE`, no conditional logic.

### XDG Directory Layout

| XDG Variable | Default | Animus Usage |
|---|---|---|
| `XDG_CONFIG_HOME` | `~/.config` | `~/.config/animus/` — faculty configs, `.env`, settings |
| `XDG_DATA_HOME` | `~/.local/share` | `~/.local/share/animus/` — Postgres, observability, instance skills |
| `XDG_CACHE_HOME` | `~/.cache` | `~/.cache/animus/` — focus scratch dirs, ephemeral |
| `XDG_DATA_DIRS` | `/usr/local/share:/usr/share` | `/usr/local/share/animus/skills/` — shared skills repo |

```
# Per-instance (under the instance's user account)
~/.config/animus/
    config.toml                    # appliance configuration
    faculties/                     # faculty TOML files
        engineer.toml
        social.toml

~/.local/share/animus/
    postgres/                      # Postgres PGDATA
    tempo/                         # trace storage
    loki/                          # log storage
    prometheus/                    # metric storage
    grafana/                       # Grafana SQLite
    skills/                        # instance skills repo (git)
    backups/                       # pg_dump outputs

~/.cache/animus/
    foci/                          # ephemeral focus working directories

# Shared across all instances (system-wide)
/usr/local/share/animus/
    skills/                        # shared skills repo (git, cloned from GitHub)
```

### Instance Setup

```sh
# Linux — create a new animus instance
sudo useradd -m -s /bin/bash cookie
sudo -u cookie mkdir -p \
    ~/.config/animus/faculties \
    ~/.local/share/animus/{postgres,tempo,loki,prometheus,grafana,skills,backups} \
    ~/.cache/animus/foci

# macOS — create a new animus instance
sudo sysadminctl -addUser cookie -password -
sudo -u cookie mkdir -p \
    ~/.config/animus/faculties \
    ~/.local/share/animus/{postgres,tempo,loki,prometheus,grafana,skills,backups} \
    ~/.cache/animus/foci
```

The animus binary and Docker Compose file live in the repo (shared). Each instance user runs them with their own XDG paths.

---

## Stack

Every animus instance runs a full observability stack:

```
animus-rs (daemon or CLI)
    → OTLP gRPC (:4317)
        → OTel Collector
            → Tempo (traces)
            → Prometheus (metrics, via remote write)
            → Loki (logs, via OTLP)
    → Grafana (:3000) — unified UI
```

| Service | Image | Internal Port | Data Path |
|---|---|---|---|
| Postgres | custom (pgmq + pgvector) | 5432 | `$XDG_DATA_HOME/animus/postgres/` |
| OTel Collector | `otel/opentelemetry-collector-contrib` | 4317, 4318 | stateless |
| Tempo | `grafana/tempo:2.7.2` | 3200 | `$XDG_DATA_HOME/animus/tempo/` |
| Loki | `grafana/loki:latest` | 3100 | `$XDG_DATA_HOME/animus/loki/` |
| Prometheus | `prom/prometheus:latest` | 9090 | `$XDG_DATA_HOME/animus/prometheus/` |
| Grafana | `grafana/grafana:latest` | 3000 | `$XDG_DATA_HOME/animus/grafana/` |

Host port mappings are configurable via env vars to avoid conflicts when multiple instances run on the same machine.

---

## Data Durability

All persistent state lives on host-mounted volumes under `$XDG_DATA_HOME/animus/`. This means:

- **Filesystem backups cover everything** — Time Machine, rsync, restic, etc.
- **`docker compose down` does not destroy data** — containers are ephemeral
- **Per-user isolation** — each instance's data is owned by its OS user
- **Standard tooling works** — `du`, `find`, `rsync`, `tar` — no Docker volume abstraction

### Backups

**Postgres (Critical — Domain State):**
```sh
docker compose exec -T postgres pg_dump -U animus animus_dev \
  > "$XDG_DATA_HOME/animus/backups/animus_$(date +%Y%m%d_%H%M%S).sql"

# 30-day retention
find "$XDG_DATA_HOME/animus/backups" -name "animus_*.sql" -mtime +30 -delete
```

**Restore:**
```sh
docker compose exec -T postgres psql -U animus animus_dev < backup_file.sql
```

**Observability data:** covered by filesystem backups. Prometheus retains 30 days, Loki retains 30 days. If lost, domain state is unaffected.

**Skills:** git repos — push to remote for backup, or covered by filesystem backups.

---

## Three-Signal Telemetry

### Traces (Tempo)

Every work item execution creates a trace:

```
work.execute
  ├── work.orient
  ├── work.engage
  │     ├── work.engage.iteration[1]
  │     │     ├── gen_ai.chat
  │     │     └── work.tool.execute[...]
  │     └── ...
  ├── work.consolidate
  └── work.recover (if needed)
```

**View in Grafana:** Explore → Tempo → Search by `service.name = animus`

### Metrics (Prometheus)

All metrics prefixed with `animus_`:

| Metric | Type | Labels | Description |
|---|---|---|---|
| `animus_work_submitted_total` | Counter | faculty, result | Work items submitted |
| `animus_work_state_transitions_total` | Counter | from, to | State transitions |
| `animus_work_unroutable_total` | Counter | faculty | Work with no matching faculty |
| `animus_queue_operations_total` | Counter | queue, operation | pgmq operations |
| `animus_memory_operations_total` | Counter | operation | Memory store operations |
| `animus_llm_tokens_total` | Counter | model, provider, direction | LLM token usage |
| `animus_operation_duration_ms_milliseconds` | Histogram | operation | Operation duration |

**View in Grafana:** Explore → Prometheus → query metric name

### Logs (Loki)

All `tracing::info!`, `warn!`, `error!` exported to Loki via the OTel log bridge. Structured fields (faculty, work_id, etc.) preserved as log labels.

**View in Grafana:** Explore → Loki → `{service_name="animus"}`

---

## Alerting

Alert rules managed in Grafana (Alerting → Alert rules). Persist in Grafana's SQLite.

### Current Alert Rules

| Alert | Query | Condition | Severity |
|---|---|---|---|
| Work with no faculty | `sum(animus_work_unroutable_total)` | > 0 | warning |

### Contact Points

Grafana → Alerting → Contact points. Supports Slack, PagerDuty, Email, Discord, webhooks. Route by label: `severity=warning` → Slack, `severity=critical` → PagerDuty.

---

## Operational Commands

### Start / Stop

```sh
docker compose up -d                   # start all services
docker compose down                    # stop (data preserved)
docker compose restart grafana         # restart one service
docker compose logs -f otel-collector  # follow logs
```

### Daemon

```sh
animus serve                           # run control plane
animus serve --faculties DIR           # custom faculty dir
```

### Work Management

```sh
animus work submit engineer bootstrap --skill tdd-implementation --params '{...}'
animus work list
animus work list --state queued
animus work show b554bcb3
```

### Database

```sh
docker compose exec postgres psql -U animus animus_dev
docker compose exec postgres psql -U animus animus_dev -c "\dt"
docker compose exec postgres psql -U animus animus_dev -c "
  SELECT state, count(*) FROM work_items GROUP BY state ORDER BY count DESC;
"
```

---

## Multi-Instance

Each instance is an OS user account. Multiple instances on one machine:

```sh
# Run as user 'cookie'
sudo -u cookie docker compose up -d

# Run as user 'dev' (your dev instance)
docker compose up -d
```

Port conflicts are avoided by configuring per-user port offsets:

| Env Var | Description |
|---|---|
| `ANIMUS_PG_PORT` | Host port for Postgres (default: 5432) |
| `ANIMUS_GRAFANA_PORT` | Host port for Grafana (default: 3000) |
| `ANIMUS_OTEL_GRPC_PORT` | Host port for OTel gRPC (default: 4317) |
| `ANIMUS_OTEL_HTTP_PORT` | Host port for OTel HTTP (default: 4318) |

Each user's `.config/animus/` contains their port configuration. Docker Compose reads `$XDG_CONFIG_HOME/animus/.env` for these.

---

## Configuration Reference

| Env Var | Default | Description |
|---|---|---|
| `XDG_CONFIG_HOME` | `~/.config` | Config root (XDG standard) |
| `XDG_DATA_HOME` | `~/.local/share` | Data root (XDG standard) |
| `XDG_CACHE_HOME` | `~/.cache` | Cache root (XDG standard) |
| `DATABASE_URL` | `postgres://animus:animus_dev@localhost:5432/animus_dev` | Postgres connection |
| `OTEL_ENDPOINT` | `http://localhost:4317` | OTLP gRPC endpoint |
| `LOG_LEVEL` | `info` | Tracing filter level |
| `ANTHROPIC_API_KEY` | — | LLM API key (secret) |
| `ANIMUS_PG_PORT` | `5432` | Host port for Postgres |
| `ANIMUS_GRAFANA_PORT` | `3000` | Host port for Grafana |
| `ANIMUS_OTEL_GRPC_PORT` | `4317` | Host port for OTel gRPC |
| `ANIMUS_OTEL_HTTP_PORT` | `4318` | Host port for OTel HTTP |
