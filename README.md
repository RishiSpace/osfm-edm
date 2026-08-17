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

# Open http://localhost:3000 and log in with the admin credentials from .env
```

## Enrolling a Device

```bash
# Fingerprint is printed at server start: "CA SHA-256 fingerprint: …"
osfm-edm-agent --server https://your-server:8080 --token <enrollment-token> --ca-fingerprint <hex>
```

## Technology Stack

| Layer | Technology |
|---|---|
| System Monitor (Linux) | Rust (procfs, netlink proc connector, fanotify) |
| System Monitor (Windows) | Planned: ETW, Win32 APIs |
| Agent | Rust (Tokio, sysinfo, rustls) |
| API Server | Rust (Axum, SQLx, Tower) |
| Console | Rust (`egui`/`eframe`) — native window, no browser |
| Web UI (optional) | TypeScript (Next.js 14) behind `docker compose --profile web` |
| Database | PostgreSQL 16 + TimescaleDB |
| Auth | JWT + TOTP; agent token + TLS pin; Ed25519 jobs |

## Development

```bash
# Prerequisites: Rust 1.78+, PostgreSQL 16 with TimescaleDB
# (Node is only needed for the optional web console)

# Backend
cargo build

# Native console (API must already be running)
cargo run -p osfm-edm-console -- --api http://localhost:8080
```

## Architecture

```
┌─────────────┐    WebSocket/mTLS    ┌──────────────┐    SSE     ┌───────────┐
│  Agent      │◄────────────────────►│  API Server  │◄──────────►│ Console   │
│  (per host) │                      │  (Axum)      │   REST     │ (egui)    │
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
- [x] **Dashboard UI** — Next.js 14 console (optional) + native `egui` console (default)

### 🚧 Pending

- [ ] **Windows System Monitor** — ETW-based process, file, network, and registry event collection
- [ ] **macOS System Monitor** — Endpoint Security framework for process, file, and network events
- [ ] **Platform Enforcers (Windows/macOS)** — OS-level policy enforcement via netsh, powercfg, pfctl, pmset
- [ ] **CI/CD** — GitHub Actions pipeline, release automation

## Current Implementation Status

**Snapshot**: August 2026

**Overall progress: ~70%** toward the full project goal (usable self-hosted platform with working dashboard, reliable alerts, active policy enforcement, and easy deployment).

The project has a working Rust backend, Linux agent, and a native `egui` console (no Chromium). Remaining work is TLS, Windows/macOS agents, and CI.

### Component Status

| Component                          | Status            | Progress | Details |
|------------------------------------|-------------------|----------|---------|
| Server Core (Axum, Auth, PKI, WS Hub) | Complete       | 95%     | Single port (8080) serves both REST API and `/ws`. Agent connections authenticated via per-device bearer tokens (mTLS deviated — see DEVIATIONS.md). |
| REST APIs (auth, devices, policies, jobs, groups, alerts, reports, software, shell, etc.) | Complete | 95% | 12 route groups. End-to-end verified: enroll → telemetry → signed job → shell SSE → RBAC. Route params fixed to axum 0.7 `:id` syntax. |
| Agent Core (enrollment, WS transport, heartbeat, telemetry, jobs) | Complete | 95% | Enrollment flow, persistent connection with `?device_id=` param, backoff, job execution all work end-to-end. |
| Remote Shell                       | Complete          | 90%     | Protocol + agent-side piped shell + server API + SSE output relay. Not a real PTY (no terminal emulation). |
| Linux System Monitor               | Implemented       | 80%     | User-space using netlink proc connector + fanotify + /proc/net. Schema mismatch fixed (migration 012). |
| Policy System                      | Functional        | 80%     | Full CRUD + assignment + WS push + compliance reporting. Linux enforcers (ufw, USB, screen lock, auto-updates, process kill) now **invoked automatically**. |
| Alerts & Notifications             | Functional        | 70%     | Alert engine evaluates rules. CRUD API for rules. Schema mismatches fixed (migration 013). SMTP/webhook/ntfy.sh notification code functional. |
| Native console (egui)              | Functional        | 80%     | `osfm-edm-console` — login, overview, devices + plots, jobs, policies, groups, alerts, reports, settings, piped shell. No browser. |
| Web UI (optional)                  | Functional        | 80%     | Next.js 14 at `:3000` via `docker compose --profile web`. Same API. |
| Cross-platform (Windows / macOS)   | Stubs             | 10%     | System monitor and enforcer modules contain only "not yet implemented" placeholders. |
| Deployment (Docker, Compose, CI)   | Functional        | 70%     | Compose starts DB + server. Native console runs on the host. Web UI is an optional compose profile. No CI yet. |

### Known Issues
- No CI/CD pipeline.
- Devices enrolled before Phase 13 lack an auth token and must be re-enrolled.
- First enroll over HTTPS needs `--ca-fingerprint` from the server log (or `--ca` / `--insecure`).

See [PROGRESS.md](PROGRESS.md) for phase-by-phase history and [ARCHITECTURE.md](ARCHITECTURE.md) for intended design.

## License

GPL-3.0
