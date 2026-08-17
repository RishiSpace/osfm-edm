"use client";

import { FormEvent, useEffect, useState } from "react";
import { useRouter } from "next/navigation";
import { Activity } from "lucide-react";
import { useAuth } from "@/lib/auth";
import { ApiError, API_URL } from "@/lib/api";
import { errorMessage } from "@/lib/format";
import { Button, Card, ErrorBanner, Field, Input } from "@/components/ui";

export default function LoginPage() {
  const { user, ready, login } = useAuth();
  const router = useRouter();
  const [username, setUsername] = useState("admin");
  const [password, setPassword] = useState("");
  const [totp, setTotp] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [apiUp, setApiUp] = useState<boolean | null>(null);

  useEffect(() => {
    if (ready && user) router.replace("/");
  }, [ready, user, router]);

  useEffect(() => {
    fetch(`${API_URL}/health`)
      .then((r) => setApiUp(r.ok))
      .catch(() => setApiUp(false));
  }, []);

  async function onSubmit(e: FormEvent) {
    e.preventDefault();
    setError(null);
    setBusy(true);
    try {
      await login(username, password, totp);
      router.replace("/");
    } catch (err) {
      if (err instanceof ApiError && err.status === 429) {
        setError(err.message);
      } else {
        setError(errorMessage(err));
      }
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="flex min-h-screen items-center justify-center px-4">
      <Card className="w-full max-w-sm shadow-glow">
        <div className="mb-6 flex items-center gap-2">
          <Activity className="h-6 w-6 text-accent" />
          <div>
            <div className="text-lg font-semibold">OSFM-EDM</div>
            <div className="text-xs text-mute">Sign in to the console</div>
          </div>
        </div>
        <form className="space-y-4" onSubmit={onSubmit}>
          <Field label="Username">
            <Input
              autoComplete="username"
              value={username}
              onChange={(e) => setUsername(e.target.value)}
              required
            />
          </Field>
          <Field label="Password">
            <Input
              type="password"
              autoComplete="current-password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              required
            />
          </Field>
          <Field label="TOTP (if enabled)">
            <Input
              inputMode="numeric"
              autoComplete="one-time-code"
              value={totp}
              onChange={(e) => setTotp(e.target.value)}
              placeholder="000000"
            />
          </Field>
          <ErrorBanner message={error} />
          <Button type="submit" className="w-full" disabled={busy}>
            {busy ? "Signing in…" : "Sign in"}
          </Button>
        </form>
        <p className="mt-4 text-xs text-mute">
          API {API_URL}:{" "}
          {apiUp == null ? "checking…" : apiUp ? "reachable" : "unreachable"}
        </p>
      </Card>
    </div>
  );
}
