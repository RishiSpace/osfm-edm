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
