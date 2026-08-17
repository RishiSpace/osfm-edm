# OSFM-EDM Dashboard

Next.js 14 console for the OSFM-EDM API.

```bash
cp ../.env.example ../.env
# API must be running on :8080
npm install
npm run dev
```

Open http://localhost:3000. Sign in with the server admin account.

`NEXT_PUBLIC_API_URL` is the URL **the browser** uses to reach the API (default `http://localhost:8080`). The server must allow that page origin via `CORS_ORIGIN` (default `http://localhost:3000`).
