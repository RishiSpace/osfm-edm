# OSFM-EDM — Open-Source Endpoint Device Management

A fully open-source, self-hosted endpoint management platform for prosumers and homelabbers.

## Features

- **Device Enrollment & Inventory** — One-command enrollment with mTLS certificates
- **Real-Time Telemetry** — CPU, RAM, disk, uptime via WebSocket + SSE
- **Policy Enforcement** — Screen lock, firewall, software restrictions
- **Software & Patch Tracking** — Installed software inventory and pending patches
- **Remote Execution** — Run scripts, push files, open remote shell
- **Kernel Monitoring** — eBPF (Linux) and KMDF (Windows) process/network/file events
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
| Kernel Driver (Linux) | Rust + aya eBPF |
| Kernel Driver (Windows) | Rust + windows-drivers-rs (KMDF) |
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
       │ eBPF / KMDF                        │ SQLx
       ▼                                    ▼
┌──────────────┐                     ┌──────────────┐
│ Kernel Driver│                     │ PostgreSQL + │
│ (optional)   │                     │ TimescaleDB  │
└──────────────┘                     └──────────────┘
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

### 🚧 Pending

- [ ] **Dashboard UI** — Next.js 14 web frontend with device overview, live telemetry charts, job console, policy editor
- [ ] **Remote Shell** — Interactive terminal sessions over WebSocket (xterm.js in browser → PTY on agent)
- [ ] **Linux Kernel Driver** — eBPF probes via `aya` for process exec, file access, network connections
- [ ] **Windows Kernel Driver** — KMDF driver via `windows-drivers-rs` for registry, USB, process monitoring
- [ ] **Platform Enforcers** — OS-level policy enforcement (iptables/ufw rules, USB disable, screensaver config)
- [ ] **Notifications** — SMTP email, webhook, and ntfy.sh alert delivery
- [ ] **Docker Images & CI** — Production Dockerfiles, GitHub Actions pipeline, release automation

## License

MIT
