# OSFM-EDM — Open-Source Endpoint Device Management

A fully open-source, self-hosted endpoint management platform for prosumers and homelabbers.

## Features

- **Device Enrollment & Inventory** — One-command enrollment with mTLS certificates
- **Real-Time Telemetry** — CPU, RAM, disk, uptime via WebSocket + SSE
- **Policy Enforcement** — Screen lock, firewall, software restrictions
- **Software & Patch Tracking** — Installed software inventory and pending patches
- **Remote Execution** — Run scripts, push files, open remote shell
- **System Monitoring** — User-space process, file, and network event tracking (procfs, netlink, fanotify)
- **Alerts & Notifications** — Configurable rules with email, webhook, ntfy.sh
- **Compliance Reporting** — Per-device policy compliance with CSV export

## Quick Start

```bash
# Clone and start the stack
git clone https://github.com/RishiSpace/osfm-edm.git
cd osfm-edm
cp .env.example .env  # Edit with your values
docker-compose up -d

# Navigate to http://localhost:8080 and log in with your admin credentials
```

## Enrolling a Device

```bash
# On the managed device (Linux/macOS):
curl -sSL https://your-server:8080/enroll.sh | bash -s -- --token <enrollment-token>

# On Windows (PowerShell):
irm https://your-server:8080/enroll.ps1 | iex -Args "--token <enrollment-token>"
```

## Technology Stack

| Layer | Technology |
|---|---|
| System Monitor (Linux) | Rust (procfs, netlink proc connector, fanotify) |
| System Monitor (Windows) | Planned: ETW, Win32 APIs |
| Agent | Rust (Tokio, sysinfo, rustls) |
| API Server | Rust (Axum, SQLx, Tower) |
| Dashboard | TypeScript (Next.js 14, Tailwind, shadcn/ui) |
| Database | PostgreSQL 16 + TimescaleDB |
| Auth | JWT + mTLS + TOTP 2FA |

## Development

```bash
# Prerequisites: Rust 1.78+, Node 20 LTS, PostgreSQL 16 with TimescaleDB

# Backend
cargo build

# Dashboard
cd dashboard && npm install && npm run dev
```

## Architecture

```
┌─────────────┐    WebSocket/mTLS    ┌──────────────┐    SSE     ┌───────────┐
│  Agent      │◄────────────────────►│  API Server  │◄──────────►│ Dashboard │
│  (per host) │                      │  (Axum)      │   REST     │ (Next.js) │
└──────┬──────┘                      └──────┬───────┘            └───────────┘
       │                                    │
       │ procfs / netlink / fanotify        │ SQLx
       ▼                                    ▼
┌──────────────┐                     ┌──────────────┐
│ System       │                     │ PostgreSQL + │
│ Monitor      │                     │ TimescaleDB  │
│ (user-space) │                     └──────────────┘
└──────────────┘
```

## Roadmap

### ✅ Completed

- [x] **Core Backend** — Axum REST API (10 route groups), JWT + TOTP auth, audit logging
- [x] **Device Enrollment** — Internal PKI (self-signed CA), one-time tokens, mTLS certificates
- [x] **WebSocket Hub** — Bidirectional agent ↔ server messaging, auto-reconnect
- [x] **Telemetry** — CPU, RAM, disk, uptime collection + TimescaleDB storage
- [x] **Policy Engine** — CRUD API, device/group assignment, compliance evaluation (firewall, encryption, USB, process blacklist)
- [x] **Remote Jobs** — Script execution (bash/sh/powershell/cmd), live stdout/stderr streaming, timeout + cancellation
- [x] **Device Groups** — CRUD + membership management
- [x] **Software Inventory** — dpkg/rpm package collection, apt/dnf patch detection
- [x] **Alerts** — Threshold-based rules (CPU/RAM/disk %), alert event tracking
- [x] **Compliance Reports** — Fleet-wide + per-device compliance summaries
- [x] **Agent** — Enrollment, heartbeat, telemetry, job execution, policy checks, inventory collection
- [x] **Linux System Monitor** — User-space process (netlink proc connector), file (fanotify), and network (/proc/net/tcp) event tracking
- [x] **Linux Platform Enforcers** — Firewall (ufw), USB storage (sysfs/modprobe), screen lock (gsettings/xset), auto-updates (apt)

### 🚧 Pending

- [ ] **Dashboard UI** — Next.js 14 web frontend with device overview, live telemetry charts, job console, policy editor
- [ ] **Remote Shell** — Interactive terminal sessions over WebSocket (xterm.js in browser → PTY on agent)
- [ ] **Windows System Monitor** — ETW-based process, file, network, and registry event collection
- [ ] **macOS System Monitor** — Endpoint Security framework for process, file, and network events
- [ ] **Platform Enforcers (Windows/macOS)** — OS-level policy enforcement via netsh, powercfg, pfctl, pmset
- [ ] **Notifications** — SMTP email, webhook, and ntfy.sh alert delivery
- [ ] **Docker Images & CI** — Production Dockerfiles, GitHub Actions pipeline, release automation

## Current Implementation Status

**Snapshot**: June 2026

**Overall progress: ~42%** toward the full project goal (usable self-hosted platform with working dashboard, reliable alerts, active policy enforcement, and easy deployment).

The project has delivered most of the Rust backend and Linux agent code corresponding to the 11 phases listed above. However, several integration gaps, schema mismatches, and the complete absence of the dashboard mean the system is not yet at the usable platform stage described in the Features and Roadmap.

### Component Status

| Component                          | Status            | Progress | Details |
|------------------------------------|-------------------|----------|---------|
| Server Core (Axum, Auth, PKI, WS Hub) | Mostly complete | 90%     | Single port (8080) serves both REST API and `/ws`. mTLS not enforced on agent connections yet. `AGENT_PORT` config value is unused. |
| REST APIs (auth, devices, policies, jobs, groups, reports, software, etc.) | Good | 85%     | All route groups implemented. Basic CRUD, job dispatch, compliance summaries, and enrollment token flows work. |
| Agent Core (enrollment, WS transport, heartbeat, telemetry, jobs) | Good | 85%     | Enrollment flow, persistent connection with backoff, and job execution (scripts, packages, **PushFile**, reboot, etc.) are functional. |
| Remote Shell                       | Partial           | 65%     | Protocol + agent-side piped shell (`/bin/sh -i` or `cmd.exe`) + server API complete. Not a real PTY. Shell output is only logged server-side (no live relay implemented). |
| Linux System Monitor               | Implemented       | 75%     | User-space using netlink proc connector + fanotify + /proc/net. Requires elevated privileges. **Broken in practice** due to `kernel_events` schema mismatch (`payload` column vs migration). |
| Policy System                      | Partial           | 60%     | Full CRUD + group/device assignment + WebSocket push + compliance *reporting* works. Linux enforcers (ufw, USB blocking, screen lock, auto-updates) are coded but **never invoked**. |
| Alerts & Notifications             | Non-functional    | 25%     | Alert engine and full notification code (SMTP via lettre, webhooks, ntfy.sh) exist in `services/`. **Schema mismatch** with `007_alerts.sql` (queries expect columns that don't exist; no `alert_rules` management API). |
| Dashboard / Web UI                 | Not started       | 0%      | 0%. No `dashboard/` directory. `docker-compose.yml` references it and will fail. |
| Cross-platform (Windows / macOS)   | Stubs             | 10%     | System monitor and enforcer modules contain only "not yet implemented" placeholders and `pending()` futures. |
| Deployment (Docker, Compose, CI)   | Non-functional    | 10%     | Compose files exist but no `Dockerfile*` are present anywhere in the repo. Quick Start will not work. |

**Overall assessment**: The core Rust foundation (server + Linux agent) is substantially built, but integration issues prevent many advertised features from working end-to-end. This aligns with the ~42% overall progress figure above.

### Known Issues Preventing Full Testing
- Agent WebSocket connections fail after enrollment (server requires `?device_id=...` query param; agent never sends it).
- Telemetry and event paths can trigger runtime SQL errors due to column mismatches.
- `docker compose up` (or the Quick Start) cannot succeed in the current state.
- No web UI exists for any of the data.

See [PROGRESS.md](PROGRESS.md) for phase-by-phase history and [ARCHITECTURE.md](ARCHITECTURE.md) for intended design (some details have drifted).

## License

GPL-3.0
