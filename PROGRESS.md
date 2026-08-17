# OSFM-EDM — Build Progress

---

## Phase 1 — Workspace Scaffold & Common Crate — COMPLETE (2026-03-14)

Built:
- Root `Cargo.toml` workspace with `osfm-edm-common`, `osfm-edm-server` (stub), `osfm-edm-agent` (stub)
- `.gitignore`, `.env.example`, `README.md`, `DEVIATIONS.md`
- `osfm-edm-common` crate: `device.rs`, `events.rs`, `policy.rs`, `jobs.rs`, `protocol.rs`
- 9 serialization round-trip tests — all passing
- No deviations

---

## Phase 2 — Database Migrations & Server Skeleton — COMPLETE (2026-03-14)

Built:
- All 10 migration SQL files (devices, metrics/timescale, policies, groups, software, jobs, alerts, users, audit_log, certificates)
- `osfm-edm-server` crate: `config.rs`, `error.rs`, `state.rs`, `main.rs` with `/health` endpoint
- Module stubs for api/ (10), services/ (5), ws/, db/queries/ (4), middleware/ (2)
- `docker-compose.yml` and `docker-compose.dev.yml`
- `cargo build -p osfm-edm-server` passes
- No deviations

---

## Phase 3 — Authentication API — COMPLETE (2026-03-14)

Built:
- `api/auth.rs`: login (bcrypt + TOTP), refresh (httpOnly cookie), logout, /me, MFA setup/verify
- `middleware/auth.rs`: JWT `AuthUser` extractor via `FromRequestParts`
- `middleware/audit.rs`: async audit logging for all POST/PATCH/PUT/DELETE requests
- First-boot admin user creation
- `cargo build -p osfm-edm-server` passes
- No deviations

---

## Phase 4 — Device Enrollment & Registry — COMPLETE (2026-03-14)

Built:
- `services/pki.rs`, `api/enroll.rs`, `api/devices.rs`
- PKI CA load/create, device cert issuance, enrollment token system
- Device CRUD + telemetry query endpoints
- No deviations

---

## Phase 5 — Agent Crate — COMPLETE (2026-03-14)

Built:
- `osfm-edm-agent` crate: CLI (clap), enrollment (HTTP), WebSocket transport (exponential backoff), telemetry (sysinfo)
- Config persistence at `~/.osfm-edm/config.toml`
- Module stubs for policy/, jobs/, kernel_bridge

---

## Phase 6 — Server WebSocket Hub — COMPLETE (2026-04-12)

Built:
- `ws/agent_hub.rs`: WS upgrade, device verification, bidirectional read/write loops
- `state.rs`: `mpsc::Sender<ServerMessage>` in `AgentConnection`, `send_to_agent`, `broadcast`
- Heartbeat → UPDATE devices, Telemetry → INSERT device_metrics, Events → INSERT kernel_events
- Job log/completion → UPDATE jobs, Compliance → UPSERT compliance_reports, Inventory → REPLACE installed_software
- Pending job dispatch + policy push on agent connect

---

## Phase 7 — Policies, Jobs & Groups — COMPLETE (2026-04-12)

Built:
- `api/policies.rs`: CRUD + assign/unassign with WS policy push to connected agents
- `api/jobs.rs`: create+dispatch via WS, list (with filters), get (with logs), cancel with agent revocation
- `api/groups.rs`: CRUD + member management (add/remove devices)
- `services/policy_engine.rs`, `services/job_queue.rs`
- Agent `jobs/executor.rs`: process spawn (bash/sh/powershell/cmd), stdout/stderr streaming, timeout
- Agent `policy/engine.rs`: firewall, USB, encryption, auto-update, process blacklist checks

---

## Phase 8 — Software Inventory — COMPLETE (2026-04-12)

Built:
- `api/software.rs`: query installed software per device
- Agent `telemetry/software.rs`: dpkg-query + rpm -qa parsers
- Agent `telemetry/patches.rs`: apt list --upgradable + dnf check-update parsers

---

## Phase 9 — Alerts & Reports — COMPLETE (2026-04-12)

Built:
- `services/alert_engine.rs`: evaluates CPU/RAM/disk rules on every telemetry insert
- `services/notifications.rs`: log-based alert dispatch (future: SMTP/webhook)
- `api/reports.rs`: fleet compliance summary + per-device compliance reports
- `011_compliance_reports.sql` migration

---

## Phase 10 — Settings & Patches — COMPLETE (2026-04-12)

Built:
- `api/settings.rs`: server config + runtime status dashboard
- `api/patches.rs`: per-device + fleet patch summary
- All 10 API routes wired in `api/mod.rs`
- Full workspace `cargo build` passes, `cargo test` passes

---

## Phase 11 — System Monitor & Platform Enforcers — COMPLETE (2026-05-22)

Refactored from kernel drivers (eBPF/KMDF) to user-space system monitoring:

Built:
- `system_monitor/mod.rs`: `MonitorConfig`, platform dispatch, event batching
- `system_monitor/linux.rs`: Process events via netlink proc connector (with /proc polling fallback), file events via fanotify, network connections via /proc/net/tcp parsing
- `system_monitor/windows.rs`: Documented stub (planned: ETW/Win32)
- `system_monitor/macos.rs`: Documented stub (planned: Endpoint Security)
- `policy/enforcers/linux.rs`: Firewall (ufw), USB storage (sysfs/modprobe), screen lock (gsettings/xfconf/xset), auto-updates (apt)
- `policy/enforcers/windows.rs`: Documented stub (planned: netsh, registry, powercfg)
- `policy/enforcers/macos.rs`: Documented stub (planned: pfctl, pmset, defaults)

Renamed across all crates:
- `KernelEvent` → `SystemEvent` (common crate)
- `KernelEventBatch` → `SystemEventBatch` (protocol)
- `PolicyRule::KernelEvents` → `PolicyRule::SystemEvents` (policy)
- Deleted `kernel_bridge.rs`, replaced with `system_monitor/` module

Agent config additions: `monitor_enabled`, `monitor_batch_interval`, `monitor_paths`
- Full workspace `cargo build` passes, `cargo test` passes (9/9 tests)

---

## Phase 12 — Integration Fixes, Alerts API & Shell Relay — COMPLETE (2026-06-21)

Fixed critical integration bugs that prevented end-to-end operation:

### Bug Fixes
- Agent WebSocket connection: added `?device_id=<uuid>` query param to WS URL (server required it, agent never sent it)
- Schema mismatch: `kernel_events.payload` — added migration 012 to add the `payload JSONB` column the server code expects
- Schema mismatch: `alert_rules` / `alert_events` — added migration 013 to add `metric`, `operator`, `threshold`, `severity`, `message`, `triggered_at` columns
- Schema mismatch: `jobs` status CHECK — added migration 014 to include `dispatched` and `completed` status values
- Fixed `agent_hub.rs` JobLog handler: was writing to nonexistent `log_output` JSONB column; now correctly INSERTs into `job_logs` table
- Fixed `agent_hub.rs` JobCompleted: `completed_at` → `finished_at` to match actual schema

### New Features
- **Alert Rules CRUD API** (`api/alerts.rs`): Full CRUD for alert rules + event listing with filters + event resolution endpoint
- **Shell Output SSE Relay**: Shell output from agents now broadcasts via `tokio::sync::broadcast` to SSE subscribers at `GET /api/v1/shell/:session_id/stream`
- **Policy Enforcer Integration**: Linux enforcers (firewall, USB, screen lock, auto-updates, process blacklist) now invoked automatically on non-compliant policy evaluation
- **USB Storage Compliance Check**: Added `lsmod` check for `usb_storage` module in policy engine
- **Process Kill Enforcement**: Blacklisted processes are now killed on policy evaluation (Linux)

### Deployment
- Created `Dockerfile.server` and `Dockerfile.agent` (multi-stage builds, non-root user)
- Fixed `docker-compose.yml`: removed nonexistent dashboard service, added `server_data` volume, removed deprecated `version` field
- Updated `.env.example` with organized sections and documentation

Full workspace `cargo build` passes, `cargo test` passes (11/11 tests), `cargo clippy` passes

---

## Phase 13 — Security Hardening & Schema Reconciliation — COMPLETE (2026-08-03)

End-to-end smoke-tested against a live PostgreSQL (enroll → WS connect → signed job → shell session → RBAC negative tests).

### Schema reconciliation (migration 015)
- `jobs`: added `created_by`, `log_output`; `job_type` now nullable — the jobs API previously failed on fresh databases
- `policies`: added `version`, `enabled` — policy create/list previously failed
- `groups` → `device_groups` and `software_inventory` → `installed_software` renames (code used these names consistently)
- `policy_assignments` reshaped from `(target_type, target_id)` → `(device_id, group_id)` with data migration + partial unique indexes — assignment API previously failed
- `devices.auth_token_hash` added for WebSocket agent authentication

### Route syntax fix (critical)
- All path parameters converted `{x}` → `:x`. **axum 0.7 (matchit 0.7) only supports colon syntax** — brace syntax is axum 0.8+. Every parameterized endpoint was silently returning 404 (braces are treated as literal path characters, no panic).

### Security hardening
- **Shell API**: authentication required (was completely unauthenticated); sessions now bound to the opening user — only owner or admin can send input, stream output, or close
- **Agent WS auth**: per-device 256-bit token issued at enrollment, sent as `Authorization: Bearer` header; server stores SHA-256 hash only. Bare `device_id` is no longer an identity
- **Ed25519 job signing**: server signs every `DispatchJob` (`services/signing.rs`, key at `data/job_signing.key`); agent verifies with the public key obtained at enrollment and rejects tampered/unsigned jobs (exit code -3). A network MITM can no longer achieve RCE via forged dispatches
- **RBAC**: `require_admin()` on all write endpoints (policies, groups, jobs, alerts, devices, enroll-token, shell); `viewer` role is read-only
- **Login rate limiting**: 5 failures / 5 min per username → HTTP 429 (in-memory sliding window)
- `Secure` cookie attribute now conditional on TLS configuration — refresh flow works on plain-HTTP dev deployments
- File permissions: server CA key, job signing key, agent config + device certs/keys all written 0600 (config dir 0700)
- `PushFile` job payload is shell-quoted (closes quote-escape injection)
- Inventory replace is transactional; agent patch data is now actually persisted (patches API reads the real `patches` table); system event batches capped at 1000/batch

### Tests
- +8 unit tests: canonical signing bytes determinism, sign/verify round-trip incl. tamper rejection + key persistence, rate limiter window behavior, shell quoting (verified against bash). **19/19 passing.**

### Known limitations (next phases)
- Transport is still plaintext HTTP/WS. Next: terminate TLS (reverse proxy or built-in rustls), then pin server identity in the agent
- Only jobs are signed; `PushPolicy`/shell messages are not individually authenticated (TLS or per-message signing closes this)
- Devices enrolled before Phase 13 have no auth token — re-enroll them

---

## Phase 14 — Dashboard UI — COMPLETE (2026-08-17)

The product is unusable without a web UI. Phase 13 closed the security/schema blockers; this phase adds the Next.js 14 dashboard specified in the README.

### Decision / Rationale / Consequence

1. **Phase identity** — Phase 13 is complete. This is Phase 14 (Dashboard), not TLS or Windows. Rationale: README lists Dashboard first among pending items; it is 0% and the reason the project exists. Consequence: TLS and cross-platform stay later phases.
2. **Next.js 14 App Router + Tailwind** — matches the published stack. Not Vite (other OSFM apps) so the documented `cd dashboard && npm run dev` path is true.
3. **No full shadcn dump** — custom lean primitives (button/input/card/badge/modal). Rationale: osfm-site pulled 40+ unused Radix packages. Consequence: fewer deps, same look.
4. **OSFM visual language** — pitch black + `#15DAE3` from osfm-site / osfm-creds-ui so the dashboard belongs in the suite.
5. **Browser talks to the API directly** (CORS + credentials). Access JWT lives in memory; refresh uses the existing httpOnly cookie. Rationale: the API already implements this; a Next BFF would duplicate auth. Consequence: CORS origin must be the dashboard URL, not `NEXT_PUBLIC_API_URL`.
6. **Backend fixes shipped with the UI** (they unblock pages, they are not a new product phase):
   - `CORS_ORIGIN` (default `http://localhost:3000`)
   - `GET /jobs/:id` includes `job_logs`
   - Alert event list SQL placeholders
   - `POST /devices/:id/request-{inventory,telemetry}`
   - Request inventory automatically on agent connect
7. **SSE shell** — `fetch` + `ReadableStream` (EventSource cannot send `Authorization`).
8. **Policy editor** — structured common rules + raw JSON for anything else. A visual rule builder is out of scope.

### Stages

| # | Goal | Success |
|---|---|---|
| 1 | Backend blockers | cargo test 19/19; filtered alerts + job logs work |
| 2 | Scaffold `dashboard/` | `npm run build` succeeds |
| 3 | Auth + chrome | login / refresh / logout / RBAC-aware nav |
| 4 | Pages | overview, devices, jobs, policies, groups, alerts, reports, settings, shell |
| 5 | Deploy/docs | compose + README + this file |
| 6 | Validate | next build + cargo test + live smoke |

### Built

- `dashboard/` Next.js 14.2 App Router: login, overview, devices + 24h charts, jobs + live logs, policies, groups, alerts, reports, inventory, settings (enroll token + TOTP), remote shell (fetch SSE).
- Access token in memory; session restore via `/auth/refresh` cookie; write actions hidden for `viewer`.
- Server: `CORS_ORIGIN` (default `http://localhost:3000`); job detail includes `job_logs`; alert event filters use `$1`-style binds; `POST /devices/:id/request-{inventory,telemetry}`; inventory pull on agent connect.
- `Dockerfile.dashboard` + compose service on `:3000`.

### Validation

- `cargo test --workspace --offline` — 19/19 pass
- `cd dashboard && npm run build` — compile + lint + types, 14 routes
- `next start :3000` — `/login` HTML contains OSFM-EDM + Sign in; `/devices|/jobs|/policies|/alerts|/settings|/reports|/groups|/software` → 200; unknown path → 404
- Live login against Postgres was **not** run (no DB on this host). Click-through of charts/jobs/shell still needs a running API.

### Still later

- TLS / server identity pin (Phase 13 leftover)
- Windows/macOS monitors
- CI
- Real PTY shell; job cancel on the agent

---

## Phase 15 — Native console (no Chromium) — COMPLETE (2026-08-17)

### Decision / Rationale / Consequence

1. **This is the next phase.** User priority is latency. A React/Next/Vite UI always pays Chromium (or equivalent) startup + JS GC. That violates the latency directive even if the pages “work.”
2. **Rejected:** Tauri / Dioxus-webview / Electron — still a web engine. **Rejected:** keep Next.js as the primary console. **Rejected:** iced (more ceremony, weaker plots) and GTK-only (weak on Windows/macOS).
3. **Chosen:** `egui` + `eframe` in `crates/osfm-edm-console`. Immediate-mode native GPU UI, one Rust binary, no Node, no browser. Plots via `egui_plot`. HTTP via blocking `reqwest` on a worker thread so the UI thread never waits on the network.
4. **ratatui considered** — lowest latency, but telemetry charts and a job/shell console are a poor fit. Homelab admins still want a window, not only a TTY.
5. **Next.js `dashboard/` stays in-tree** as an optional web console (`docker compose --profile web`). It is no longer the documented default. One product, two clients on the same API — do not fork business logic into the UI.
6. **Auth:** same JWT + refresh cookie as Phase 14. Cookie jar on the native HTTP client. Token never logged.

### Stages

| # | Goal | Success |
|---|---|---|
| 1 | Crate + API client | `cargo build -p osfm-edm-console` |
| 2 | Screens | login, overview, devices+plot, jobs, policies, groups, alerts, reports, settings, shell |
| 3 | Docs / compose | native is default; Next.js behind `web` profile |
| 4 | Validate | workspace tests still pass; console compiles |

### Built

- `crates/osfm-edm-console`: egui/eframe window, worker-thread HTTP, cookie jar + JWT refresh.
- Screens: login, overview (15s poll), devices, device + CPU/RAM plot, jobs/dispatch, policies, groups, alerts, reports, settings/enroll token, piped shell + SSE.
- Compose: Next.js dashboard moved to profile `web`. Default stack is DB + server; console runs on the host.

### Validation

- `cargo build -p osfm-edm-console` — success
- `cargo test --workspace` — includes new envelope parse tests
- GUI click-through not automated (needs a display + live API)

---

## Phase 16 — TLS + gap close — COMPLETE (2026-08-17)

### Decision / Rationale / Consequence

1. **Built-in rustls, not “proxy only.”** Homelab users will expose :8080. Auto-issue a server cert from the existing internal CA. `ALLOW_INSECURE_HTTP=1` is the explicit escape hatch.
2. **Pin the CA, do not accept any cert.** Enrollment: `--ca PATH` or `--ca-fingerprint HEX` (TOFU against `/ca.crt`). `--insecure` is opt-in and logged. After enroll, `ca.crt` on disk is the trust anchor for HTTP and WSS.
3. **Fingerprint is SHA-256 of CA DER**, not PEM text (the old `fingerprint()` hashed PEM bytes).
4. **Gaps in the same phase** because they are already specified holes, not a new product: revoke token, job cancel+timeout, alert cooldown, hashed enroll tokens, JWT role from DB, PTY shell, screen-lock check, CollectInventory, enroll scripts, console job target + TOTP + log poll, license/lockfile/docs/compose URL.

### Stages

| # | Goal | Success |
|---|---|---|
| 1 | TLS server + CA pin on agent/console | HTTPS default; enroll refuses unknown CA |
| 2 | Security gaps | revoke disconnects; cancel kills; alerts don't storm; tokens hashed |
| 3 | Product gaps | PTY, inventory job, enroll.{sh,ps1}, console polish |
| 4 | Hygiene | GPL in Cargo.toml; Cargo.lock tracked; ARCHITECTURE current |

### Built
- rustls HTTPS/WSS; CA-signed `data/server.crt`; `/ca.crt` + `/ca.fingerprint`; `ALLOW_INSECURE_HTTP` escape hatch
- Agent/console pin CA via `--ca` / `--ca-fingerprint`; `--insecure` opt-in; WSS uses pinned rustls roots; WS backoff resets
- Device revoke clears `auth_token_hash` and drops the WS handle
- Job cancel kills the process; default 300s timeout; CollectInventory sends a real report
- Alerts skip insert while an unresolved event exists for the same rule+device
- Enrollment tokens stored as SHA-256; JWT role re-read from DB
- PTY shell (`portable-pty`); Linux screen-lock check via gsettings (fail closed)
- `GET /enroll.sh` and `/enroll.ps1`; `scripts/` copies
- Console: HTTPS default, job log poll, TOTP setup
- Workspace license GPL-3.0-only; `Cargo.lock` tracked; compose `SERVER_URL=https://localhost:8080`

### Validation
- `cargo build -p osfm-edm-server -p osfm-edm-agent -p osfm-edm-console`
- `cargo test --workspace`
