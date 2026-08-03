# OSFM-EDM — Stack Deviations

This file documents any deviations from the originally specified technology stack (see Section 2 of the build instructions). Each entry explains what was changed, why, and what would be needed to revert to the original approach.

---

## 1. mTLS agent authentication → per-device token + Ed25519 job signing (2026-08-03)

**Original design (ARCHITECTURE.md):** agents connect over a dedicated WebSocket port authenticated via per-device mTLS certificates issued by the internal CA.

**Implemented instead:** a random 256-bit per-device token issued at enrollment (only its SHA-256 hash is stored server-side) presented as an `Authorization: Bearer` header on the WebSocket handshake, plus **Ed25519 signatures on every dispatched job** which agents verify using the server's public key (obtained at enrollment).

**Why:** full rustls client-certificate plumbing is disproportionate complexity for the homelab target. A secret identity credential + signed jobs cover the critical threat (forged server → RCE on managed endpoints) without the operational burden. Enrollment-issued certificates are still generated and stored for future use.

**To revert:** terminate TLS with client-cert verification in `ws/agent_hub.rs` (the CA already exists in `services/pki.rs`), extracting the device ID from the certificate CN. Keep job signing regardless — defense in depth.

## 2. Server-terminated TLS → deferred (2026-08-03)

The server binds plain HTTP/WS on `SERVER_PORT`. `TLS_CERT_PATH`/`TLS_KEY_PATH` are configuration scaffolding: they currently only toggle the `Secure` attribute on the refresh cookie. For production, terminate TLS at a reverse proxy (Caddy/nginx/Traefik) in front of the server. Built-in rustls termination remains an option for a future phase.

## 3. Route parameter syntax — axum 0.7 (`:id`), not axum 0.8 (`{id}`) (2026-08-03)

Path parameters are written `:param`. The `{param}` brace syntax requires **axum 0.8+** (matchit 0.8); brace literals in axum 0.7 are treated as literal path characters and silently produce 404s. If the workspace upgrades to axum 0.8, all route literals must be migrated to `{param}` and validated (**axum 0.8 rejects colon-style params**).
