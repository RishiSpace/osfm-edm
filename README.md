# OSFM-EDM — Open-Source Endpoint Device Management

A fully open-source, self-hosted endpoint management platform for prosumers and homelabbers. Manage 2–50 devices (Windows, Linux, macOS) from a single web dashboard.

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
git clone https://github.com/RishiSpace/osfm_edm.git
cd osfm_edm
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

## License

MIT
