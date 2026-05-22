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
