"use client";

import { useEffect, useState } from "react";
import { PageHeader } from "@/components/chrome";
import { Button, Card, ErrorBanner, Field, Input } from "@/components/ui";
import { get, post } from "@/lib/api";
import { useAuth } from "@/lib/auth";
import { errorMessage, fmtTime } from "@/lib/format";
import type { ServerStatus } from "@/lib/types";

type Settings = {
  server_port: number;
  agent_port: number;
  server_url: string;
  tls_configured: boolean;
  ca_initialized: boolean;
};

export default function SettingsPage() {
  const { isAdmin, user, reload } = useAuth();
  const [settings, setSettings] = useState<Settings | null>(null);
  const [status, setStatus] = useState<ServerStatus | null>(null);
  const [token, setToken] = useState<string | null>(null);
  const [expires, setExpires] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [mfaUrl, setMfaUrl] = useState<string | null>(null);
  const [mfaCode, setMfaCode] = useState("");

  useEffect(() => {
    Promise.all([get<Settings>("/api/v1/settings"), get<ServerStatus>("/api/v1/settings/status")])
      .then(([s, st]) => {
        setSettings(s);
        setStatus(st);
      })
      .catch((err) => setError(errorMessage(err)));
  }, []);

  return (
    <>
      <PageHeader title="Settings" subtitle="Server identity and enrollment" />
      <ErrorBanner message={error} />
      <div className="grid gap-4 lg:grid-cols-2">
        <Card>
          <h2 className="mb-3 text-sm font-medium">Runtime</h2>
          <dl className="space-y-2 text-sm">
            <Row k="Version" v={status?.version ?? "—"} />
            <Row k="Public URL" v={settings?.server_url ?? "—"} />
            <Row k="API port" v={String(settings?.server_port ?? "—")} />
            <Row k="Configured agent port" v={`${settings?.agent_port ?? "—"} (unused; WS is on the API port)`} />
            <Row k="TLS flag" v={settings?.tls_configured ? "set" : "plain HTTP"} />
            <Row k="Internal CA" v={settings?.ca_initialized ? "ready" : "missing"} />
            <Row k="Users" v={String(status?.total_users ?? "—")} />
          </dl>
        </Card>
        <Card>
          <h2 className="mb-3 text-sm font-medium">Enrollment token</h2>
          <p className="mb-3 text-sm text-mute">
            One-time token, 24h expiry. On the device:{" "}
            <code className="font-mono text-xs">
              osfm-edm-agent --server http://&lt;api-host&gt;:8080 --token &lt;token&gt;
            </code>
          </p>
          {isAdmin ? (
            <Button
              onClick={() =>
                post<{ token: string; expires_at: string }>("/api/v1/enroll/token")
                  .then((t) => {
                    setToken(t.token);
                    setExpires(t.expires_at);
                    setError(null);
                  })
                  .catch((err) => setError(errorMessage(err)))
              }
            >
              Generate token
            </Button>
          ) : (
            <p className="text-sm text-mute">Admin role required.</p>
          )}
          {token && (
            <div className="mt-3">
              <Field label={`Expires ${fmtTime(expires)}`}>
                <Input readOnly value={token} onFocus={(e) => e.currentTarget.select()} />
              </Field>
              <Button
                variant="outline"
                className="mt-2"
                onClick={() => navigator.clipboard.writeText(token)}
              >
                Copy
              </Button>
            </div>
          )}
        </Card>
        <Card>
          <h2 className="mb-3 text-sm font-medium">Two-factor auth</h2>
          <p className="mb-3 text-sm text-mute">
            {user?.totp_enabled ? "TOTP is enabled on this account." : "TOTP is optional."}
          </p>
          <Button
            variant="outline"
            onClick={() =>
              post<{ secret: string; otpauth_url: string }>("/api/v1/auth/mfa/setup")
                .then((d) => {
                  setMfaUrl(d.otpauth_url);
                  setError(null);
                })
                .catch((err) => setError(errorMessage(err)))
            }
          >
            Start TOTP setup
          </Button>
          {mfaUrl && (
            <div className="mt-3 space-y-2">
              <p className="break-all font-mono text-xs text-mute">{mfaUrl}</p>
              <Field label="Verification code">
                <Input value={mfaCode} onChange={(e) => setMfaCode(e.target.value)} />
              </Field>
              <Button
                onClick={() =>
                  post("/api/v1/auth/mfa/verify", { code: mfaCode })
                    .then(() => {
                      setMfaUrl(null);
                      return reload();
                    })
                    .catch((err) => setError(errorMessage(err)))
                }
              >
                Enable TOTP
              </Button>
            </div>
          )}
        </Card>
      </div>
    </>
  );
}

function Row({ k, v }: { k: string; v: string }) {
  return (
    <div className="flex justify-between gap-4">
      <dt className="text-mute">{k}</dt>
      <dd className="text-right">{v}</dd>
    </div>
  );
}
