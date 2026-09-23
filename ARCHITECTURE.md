# OSFM-EDM — Architecture Guide

> **Open-Source Endpoint Device Management**
> Self-hosted platform for managing 2–50 devices (Windows, Linux, macOS) from a single web dashboard.

---

## Table of Contents

1. [High-Level Architecture](#high-level-architecture)
2. [Repository Layout](#repository-layout)
3. [Component Deep-Dive](#component-deep-dive)
4. [Data Flow](#data-flow)
5. [Database Schema](#database-schema)
6. [Security Model](#security-model)
7. [Configuration Reference](#configuration-reference)
8. [Getting Started](#getting-started)
9. [Development Workflow](#development-workflow)

---

## High-Level Architecture

```
┌──────────────┐        HTTPS / REST         ┌─────────────────────┐
│   Dashboard  │ ◄──────────────────────────► │                     │
│  (Next.js)   │        :3000 → :8080        │   osfm-edm-server   │
└──────────────┘                              │     (Axum API)      │
                                              │       :8080         │
┌──────────────┐  HTTPS/WSS + device token   │                     │
│  osfm-edm    │ ◄──────────────────────────► │   WebSocket Hub     │
│   agent      │        :8080 /ws             │   /ws (same port)   │
│  (per device) │                              │                     │
└──────┬───────┘                              └──────────┬──────────┘
       │                                                  │
       │ sysinfo / user-space monitor         │ sqlx
       ▼                                                  ▼
  ┌──────────┐                                 ┌──────────────────┐
  │  OS / HW │                                 │  TimescaleDB     │
  │  (root)  │                                 │  (PostgreSQL)    │
  └──────────┘                                 └──────────────────┘
```

The platform has **four main components**:

| Component | Language | Purpose |
|-----------|----------|---------|
| `osfm-edm-common` | Rust (lib) | Shared types and protocol definitions |
| `osfm-edm-server` | Rust (bin) | Axum REST API + WebSocket hub |
| `osfm-edm-agent` | Rust (bin) | Per-device agent daemon |
| Console | Rust / egui | Native admin UI (`osfm-edm-console`) |
| Dashboard | Next.js 14 (optional) | Browser UI (`dashboard/`, compose profile `web`) |

---

## Repository Layout

```
osfm-edm/
├── Cargo.toml                    # Workspace root
├── .env.example                  # Environment variable template
├── docker-compose.yml            # Production stack (DB + server + dashboard)
├── docker-compose.dev.yml        # Dev stack (DB only)
├── migrations/                   # SQL migrations (run by sqlx on startup)
│   ├── 001_init_devices.sql      # devices table
│   ├── 002_timescale_metrics.sql # device_metrics hypertable + kernel_events
│   ├── 003_policies.sql          # policies + policy_assignments
│   ├── 004_groups.sql            # device_groups + group_members
│   ├── 005_software.sql          # installed_software
│   ├── 006_jobs.sql              # jobs table
│   ├── 007_alerts.sql            # alert_rules + alert_events (+patches)
│   ├── 008_users.sql             # users + refresh_tokens + enrollment_tokens
│   ├── 009_audit_log.sql         # audit_log
│   ├── 010_certificates.sql      # certificates (device cert tracking)
│   ├── 011_compliance_reports.sql# compliance_reports
│   ├── 012_fix_kernel_events_schema.sql  # payload JSONB
│   ├── 013_fix_alerts_schema.sql  # typed rule/event columns
│   ├── 014_fix_jobs_status.sql    # dispatched/completed statuses
│   ├── 015_reconcile_schema_and_security.sql  # renames + auth_token_hash
│   ├── 016_hash_enrollment_tokens.sql  # SHA-256 enroll tokens
│   └── 017_retention.sql         # telemetry/events/logs/alerts/audit retention
│
└── crates/
    ├── osfm-edm-common/          # Shared types crate
    │   ├── src/
    │   │   ├── lib.rs            # Module re-exports
    │   │   ├── device.rs         # DeviceInfo, OsType, DeviceStatus, Enrollment types
    │   │   ├── events.rs         # SystemEvent (process, file, network, registry)
    │   │   ├── policy.rs         # PolicyDefinition, PolicyRule variants, ComplianceReport
    │   │   ├── jobs.rs           # JobPayload, JobStatus, ShellType (+ signing bytes)
    │   │   └── protocol.rs       # ServerMessage / AgentMessage WebSocket envelopes
    │   └── tests/
    │       └── serialization_tests.rs  # serde round-trip tests
    │
    ├── osfm-edm-server/          # API server crate
    │   └── src/
    │       ├── main.rs           # Entrypoint: DB connect, migrate, PKI init, Axum serve
    │       ├── config.rs         # Env-based configuration
    │       ├── error.rs          # ApiError enum with HTTP status mapping
    │       ├── state.rs          # AppState (PgPool, Config, CA, connected agents)
    │       ├── api/
    │       │   ├── mod.rs        # API router: 12 groups under /api/v1
    │       │   ├── auth.rs       # Login, logout, refresh, /me, MFA setup/verify
    │       │   ├── enroll.rs     # Enrollment token + device enrollment with cert
    │       │   ├── devices.rs    # Device CRUD + telemetry query
    │       │   ├── groups.rs     # Device groups + members
    │       │   ├── policies.rs   # Policy CRUD + assign/unassign + push
    │       │   ├── jobs.rs       # Job create/dispatch/list/logs/cancel
    │       │   ├── software.rs   # Software inventory reads
    │       │   ├── patches.rs    # Patch status reads
    │       │   ├── reports.rs    # Compliance reports
    │       │   ├── settings.rs   # Server config + status
    │       │   ├── shell.rs      # PTY shell open/input/stream/close
    │       │   └── alerts.rs     # Alert rules + events + resolve
    │       ├── middleware/
    │       │   ├── auth.rs       # JWT AuthUser extractor (FromRequestParts)
    │       │   └── audit.rs      # Async audit log for state-changing requests
    │       ├── services/
    │       │   ├── pki.rs        # Internal CA: generate, persist, issue device certs
    │       │   ├── signing.rs    # Ed25519 job signing
    │       │   ├── alert_engine.rs   # Telemetry rule evaluation
    │       │   ├── job_queue.rs      # Pending-job dispatch
    │       │   ├── notifications.rs  # SMTP/webhook/ntfy dispatch
    │       │   └── policy_engine.rs  # Policy push to devices
    │       ├── ws/
    │       │   └── agent_hub.rs  # WebSocket hub (token auth, dispatch, relay)
    │       └── db/
    │           └── queries/      # Reserved query modules (SQL currently inline)
    │
    ├── osfm-edm-console/         # Native egui console (default UI)
    │
    └── osfm-edm-agent/           # Agent crate
        └── src/
            ├── main.rs           # CLI, enrollment-or-load, heartbeat loop
            ├── config.rs         # TOML config (~/.osfm-edm/config.toml)
            ├── enrollment.rs     # HTTP enrollment → save certs + config
            ├── tls.rs            # CA pin / fingerprint helpers
            ├── transport/
            │   ├── websocket.rs  # WSS connection with exponential backoff reconnect
            │   └── protocol.rs   # Message serialize/deserialize helpers
            ├── telemetry/
            │   ├── system.rs     # CPU, RAM, disk, uptime via sysinfo
            │   ├── software.rs   # Package inventory (dpkg/rpm, winget, brew)
            │   └── patches.rs    # Patch status (apt/dnf, winget, brew)
            ├── policy/
            │   ├── engine.rs     # Policy evaluation + compliance reports
            │   └── enforcers/    # Platform enforcers (linux, windows, macos)
            ├── jobs/
            │   ├── executor.rs   # Job execution + output streaming
            │   └── registry.rs   # In-flight jobs for cancel
            ├── shell/            # PTY shell sessions (portable-pty)
            └── system_monitor/   # User-space monitors (linux, windows, macos)
```

---

## Component Deep-Dive

### osfm-edm-common

The **shared types** crate defines the contract between all components. Everything communicated over the wire or stored in the database has a corresponding type here.

**Key types:**

- **`ServerMessage` / `AgentMessage`** — JSON-tagged enums forming the WebSocket protocol. Every message is `{ "msg_type": "...", ... }` for easy parsing on both sides.
- **`SystemEvent`** — tagged union of `ProcessStarted`, `ProcessExited`, `FileAccessed`, `NetworkConnected`, and `RegistryChanged` events from user-space monitors.
- **`PolicyRule`** — variants like `Firewall`, `UsbStorage`, `ScreenLock`, `OsUpdate`, `ProcessBlacklist`, and `SystemEvents` that define enforceable rules.
- **`JobPayload`** — `RunScript`, `InstallPackage`, `UninstallPackage`, `PushFile`, `Reboot`, `CollectInventory`, `RunPatchUpdate` for remote execution.

### osfm-edm-server

The **Axum-based API server** is the brain of the platform. It serves REST +
`/ws` on a single port:

| Port | Purpose | Auth |
|------|---------|------|
| `:8080` | HTTPS REST + WSS `/ws` | JWT (humans), device token (agents) |

**Startup sequence:**
1. Load config from environment variables
2. Connect to PostgreSQL (via `sqlx`, pool max 10)
3. Run migrations (`migrations/` directory, currently 001–017)
4. Initialize PKI — load or generate a self-signed CA at `data/ca.crt`
5. Create default admin user if `users` table is empty
6. Bind Axum HTTPS with CORS, 1 MiB request body limit, tracing, and audit middleware

**Authentication flow:**
```
Client                         Server
  │                              │
  ├── POST /auth/login ────────► │  Verify bcrypt hash + optional TOTP
  │   { username, password }     │
  │                              │
  ◄── 200 + JWT + Set-Cookie ──┤  JWT (15min) in body; refresh (7d) in httpOnly cookie
  │                              │
  ├── POST /auth/refresh ──────► │  Validate refresh token hash in DB
  │   (cookie: refresh_token)    │
  │                              │
  ◄── 200 + new JWT ───────────┤  Issues new JWT, same refresh cookie
```

**Device enrollment flow:**
```
Admin (browser)                 Server                        Agent
  │                              │                              │
  ├── POST /enroll/token ──────► │                              │
  │   (requires JWT)             │                              │
  ◄── { token: "abc-123" } ────┤                              │
  │                              │                              │
  │  (admin gives token to agent operator)                     │
  │                              │                              │
  │                              ◄── POST /enroll ─────────────┤
  │                              │   { token, hostname, os }    │
  │                              │                              │
  │                              │  1. Validate token           │
  │                              │  2. INSERT device            │
  │                              │  3. Issue mTLS cert (PKI)    │
  │                              │  4. Store cert in DB         │
  │                              │  5. Mark token used          │
  │                              │                              │
  │                              ├── { device_id, certs } ────►│
  │                              │                              │
  │                              │                  Write certs to ~/.osfm-edm/
  │                              │                  Write config.toml
```

### osfm-edm-agent

The **per-device daemon** runs on every managed endpoint. It:

1. **Enrolls** on first run (HTTPS POST with one-time token + CA pin → receives device cert + token + server pubkey)
2. **Connects** to the server via WSS with automatic exponential backoff reconnection (1s → 2s → 4s → ... → 60s max)
3. **Sends heartbeat + telemetry** every 60 seconds (configurable):
   - CPU usage (%), RAM used/total (MB), disk used/total (GB), uptime (seconds)
4. **Monitors system events** via user-space APIs (if enabled):
   - Linux: processes via netlink proc connector (/proc fallback), files via fanotify, network via /proc/net/tcp
   - Windows/macOS: processes via sysinfo snapshots, files via directory scans, network via netstat polling
5. **Handles server messages**: policy pushes, signed job dispatches, inventory requests, PTY shell sessions

**Agent config** is stored at `~/.osfm-edm/config.toml`:
```toml
server_url = "https://osfm-edm.local:8080"
device_id = "550e8400-..."
cert_path = "/home/user/.osfm-edm/device.crt"
key_path = "/home/user/.osfm-edm/device.key"
ca_path = "/home/user/.osfm-edm/ca.crt"
heartbeat_interval = 60
telemetry_interval = 60
monitor_enabled = true
monitor_batch_interval = 5
monitor_paths = ["/"]
```

### Internal PKI

The server acts as its own **Certificate Authority**:

- On first startup, generates a self-signed CA keypair → `data/ca.crt` + `data/ca.key`
- On subsequent startups, loads the existing CA from disk
- When a device enrolls, issues a device certificate with `CN=device:<uuid>`
- Device certs are used for mTLS on the WebSocket port — the server extracts the device ID from the certificate's Common Name

---

## Data Flow

```
Agent                           Server                         Dashboard
  │                               │                               │
  │──── Heartbeat ───────────────►│                               │
  │     { agent_version }         │  UPDATE devices.last_seen     │
  │                               │                               │
  │──── TelemetryReport ────────►│                               │
  │     { cpu, ram, disk }        │  INSERT device_metrics        │
  │                               │  (TimescaleDB hypertable)     │
  │                               │                               │
  │──── KernelEventBatch ───────►│                               │
  │     [ process, file, net ]    │  INSERT kernel_events         │
  │                               │                               │
  │                               │◄── GET /devices ──────────────│
  │                               │──► [ device list ] ──────────►│
  │                               │                               │
  │                               │◄── GET /devices/:id/telemetry │
  │                               │──► [ metric time series ] ───►│
  │                               │                               │
  │◄── DispatchJob ──────────────│◄── POST /jobs ─────────────── │
  │    { job_id, payload, sig }   │                               │
  │                               │                               │
  │──── JobLog ─────────────────►│                               │
  │──── JobCompleted ───────────►│──► (stream to dashboard) ────►│
```

---

## Database Schema

**TimescaleDB** (PostgreSQL + time-series extensions), migrations 001–017:

| Table | Purpose | Notes |
|-------|---------|-------|
| `devices` | Device registry | hostname, os, status, last_seen, `auth_token_hash` |
| `device_metrics` | Time-series telemetry | **Hypertable** — CPU, RAM, disk, uptime (nullable; 30-day retention) |
| `kernel_events` | System events | **Hypertable** — process, file, network, registry + `payload` JSONB (30-day retention) |
| `policies` | Policy definitions | JSON rules, `version`, `enabled` |
| `policy_assignments` | Policy → device/group | Explicit `device_id` / `group_id` (exactly one set) |
| `device_groups` | Logical grouping | Name + description |
| `installed_software` | Software inventory | Per-device package list |
| `patches` | Pending patches | Per-device patch list |
| `jobs` | Remote execution jobs | Payload, status (`pending`/`dispatched`/`running`/`completed`/`done`/`failed`/`cancelled`), `created_by`, `log_output` |
| `job_logs` | Job output lines | Capped at 500/job by retention |
| `alert_rules` / `alert_events` | Alerting system | Typed rule columns + severity/message/triggered_at (resolved rows kept 90d) |
| `users` | Admin accounts | bcrypt password, TOTP secret, role |
| `refresh_tokens` | JWT refresh tokens | Hashed (SHA-256), revocable |
| `enrollment_tokens` | One-time enrollment | SHA-256 hashed, 24h expiry, single-use |
| `certificates` | Device certs | PEM, fingerprint, revocation status |
| `compliance_reports` | Compliance state | Per device+policy, upserted |
| `audit_log` | All state-changing API calls | User, action, IP (direct peer), timestamp (1-year retention) |

---

## Security Model

| Layer | Mechanism |
|-------|-----------|
| Dashboard → Server | JWT access tokens (15min) + httpOnly refresh cookie (7d) |
| Authorization | `require_admin()` gate on all write endpoints; `viewer` role is read-only |
| Agent → Server | HTTPS/WSS (rustls). Agents pin the internal CA (`--ca` / `--ca-fingerprint`). Per-device 256-bit token (Bearer, SHA-256 at rest). Device certs are issued but not used for the handshake — see DEVIATIONS.md |
| Job integrity | Ed25519 signature on every `DispatchJob`, verified by the agent before execution |
| Passwords | bcrypt hashing |
| 2FA | TOTP (RFC 6238) — optional per-user |
| Login protection | Sliding-window rate limiting (5 failures / 5 min per username → HTTP 429) |
| Refresh tokens | SHA-256 hashed in DB, revocable on logout |
| Audit trail | Every POST/PATCH/PUT/DELETE logged with user, action, IP |
| Enrollment | One-time tokens with 24h expiry |
| Key material | CA key, signing key, agent secrets written with 0600 permissions |
| Transport | rustls HTTPS/WSS on `SERVER_PORT`. `ALLOW_INSECURE_HTTP=1` is an explicit opt-out. |

**Route parameter syntax note:** this codebase pins **axum 0.7**, whose router expects `:param` (not `{param}` — that syntax requires axum 0.8+). See DEVIATIONS.md #3.

---

## Configuration Reference

All server configuration is via **environment variables** (see `.env.example`):

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `DATABASE_URL` | ✅ | — | PostgreSQL connection string |
| `JWT_SECRET` | ✅ | — | ≥32 char secret for JWT signing |
| `SERVER_PORT` | | `8080` | HTTPS REST + WSS port |
| `SERVER_URL` | | `https://localhost:8080` | Public URL given to agents |
| `ADMIN_USERNAME` | | `admin` | First-boot admin username |
| `ADMIN_PASSWORD` | | `admin` | First-boot admin password |
| `TLS_CERT_PATH` | | — | Custom TLS cert (optional) |
| `TLS_KEY_PATH` | | — | Custom TLS key (optional) |
| `ALLOW_INSECURE_HTTP` | | unset | `1`/`true` = plaintext HTTP (dev only) |
| `CORS_ORIGIN` | | `http://localhost:3000` | Browser origin allowed by CORS (the dashboard) |
| `NEXT_PUBLIC_API_URL` | | `http://localhost:8080` | API URL baked into the dashboard JS bundle |
| `SMTP_HOST` / `SMTP_PORT` / `SMTP_USER` / `SMTP_PASSWORD` / `SMTP_FROM` | | — / `587` / — / — / `osfm-edm@localhost` | Email notifications |
| `WEBHOOK_URL` | | — | JSON POST target for alerts |
| `NTFY_TOPIC` / `NTFY_SERVER` | | — / `https://ntfy.sh` | Push notifications |

`AGENT_PORT` was removed: REST and `/ws` share `SERVER_PORT`. If set, the
server logs a warning and ignores it.

---

## Getting Started

### Prerequisites

- **Rust** ≥ 1.75 (install via [rustup](https://rustup.rs))
- **Docker + Docker Compose** (for the database)
- **PostgreSQL 16 + TimescaleDB** (provided by `docker-compose.dev.yml`)

### 1. Clone & configure

```bash
git clone https://github.com/RishiSpace/osfm-edm.git
cd osfm-edm
cp .env.example .env
# Edit .env — at minimum change JWT_SECRET and ADMIN_PASSWORD
```

### 2. Start the database

```bash
docker compose -f docker-compose.dev.yml up -d
```

This launches a TimescaleDB container on port 5432 with user `osfm_edm`, password `secret`, database `osfm_edm`.

### 2b. Production compose

`docker-compose.yml` runs DB + server and **requires secrets in the
environment** — nothing sensitive ships by default:

```bash
export POSTGRES_PASSWORD="$(openssl rand -hex 32)"
export JWT_SECRET="$(openssl rand -hex 32)"
export ADMIN_PASSWORD="$(openssl rand -hex 16)"
docker compose up -d
```

### 3. Build and run the server

```bash
# Source the .env file (fish shell)
export (cat .env | grep -v '^#' | xargs -L 1)

# Or for bash:
# set -a; source .env; set +a

cargo run -p osfm-edm-server
```

On first start, the server will:
1. Connect to PostgreSQL
2. Run migrations 001–017
3. Generate a CA certificate at `data/ca.crt`
4. Create the default admin user
5. Start listening HTTPS on `:8080` (REST + `/ws`)

### 4. Verify the server is running

```bash
curl -sk https://localhost:8080/health
# → {"data":{"status":"ok","version":"0.1.0"},"error":null}
```

### 5. Enroll a device (agent)

```bash
# 1. Login and get a JWT
TOKEN=$(curl -sk -X POST https://localhost:8080/api/v1/auth/login \
  -H 'Content-Type: application/json' \
  -d '{"username":"admin","password":"your_password"}' \
  | jq -r '.data.access_token')

# 2. Create an enrollment token
ENROLL_TOKEN=$(curl -sk -X POST https://localhost:8080/api/v1/enroll/token \
  -H "Authorization: Bearer $TOKEN" \
  | jq -r '.data.token')

# 3. Run the agent with the enrollment token (fingerprint from the server log)
cargo run -p osfm-edm-agent -- --server https://localhost:8080 --token "$ENROLL_TOKEN" --ca-fingerprint <hex>
```

After enrollment, the agent stores its config at `~/.osfm-edm/config.toml` and will auto-reconnect on subsequent runs (no `--token` needed).

### 6. Run tests

```bash
cargo test                      # All workspace tests
cargo test -p osfm-edm-common   # Just the common crate
cargo clippy --workspace        # Lint (CI denies warnings)
```

---

## Development Workflow

### Adding a new API endpoint

1. Create or edit a handler in `crates/osfm-edm-server/src/api/<module>.rs`
2. Add the route in the module's `router()` function
3. Wire the sub-router in `api/mod.rs` via `.nest()`
4. Use the `AuthUser` extractor for protected endpoints
5. Use `ApiResult<impl IntoResponse>` as the return type

### Adding a new migration

1. Create `migrations/NNN_description.sql` with idempotent SQL
2. Migrations run automatically on server start via `sqlx::migrate!()`

### Adding a new agent capability

1. Add a new `ServerMessage` variant in `osfm-edm-common/src/protocol.rs`
2. Add the corresponding `AgentMessage` response variant
3. Handle the new message in `osfm-edm-agent/src/main.rs` → `handle_server_message()`
4. Add the server-side dispatch in the WebSocket hub

### Project conventions

- **No panics in production code** — all errors use `Result` / `ApiError`
- **Stub modules** are marked with comments indicating planned implementation approach
- **Database access** is via raw SQL with `sqlx::query_as` — no ORM
- **Crate naming**: `osfm-edm-*` (Cargo names) / `osfm_edm_*` (Rust imports)
