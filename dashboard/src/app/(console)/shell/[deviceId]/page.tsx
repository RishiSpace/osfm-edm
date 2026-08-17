"use client";

import { FormEvent, useEffect, useRef, useState } from "react";
import { useParams } from "next/navigation";
import { PageHeader } from "@/components/chrome";
import { Button, ErrorBanner, Input } from "@/components/ui";
import { API_URL, del, getAccessToken, post } from "@/lib/api";
import { useAuth } from "@/lib/auth";
import { errorMessage } from "@/lib/format";

export default function ShellPage() {
  const { deviceId } = useParams<{ deviceId: string }>();
  const { isAdmin } = useAuth();
  const [sessionId, setSessionId] = useState<string | null>(null);
  const [lines, setLines] = useState<string>("");
  const [input, setInput] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [closed, setClosed] = useState(false);
  const endRef = useRef<HTMLDivElement>(null);
  const abortRef = useRef<AbortController | null>(null);

  useEffect(() => {
    endRef.current?.scrollIntoView({ block: "end" });
  }, [lines]);

  useEffect(() => {
    return () => {
      abortRef.current?.abort();
    };
  }, []);

  async function openSession() {
    setError(null);
    setClosed(false);
    setLines("");
    try {
      const data = await post<{ session_id: string; device_id: string }>(
        `/api/v1/shell/${deviceId}`,
      );
      setSessionId(data.session_id);
      await stream(data.session_id);
    } catch (err) {
      setError(errorMessage(err));
    }
  }

  async function stream(sid: string) {
    const token = getAccessToken();
    if (!token) {
      setError("Not signed in");
      return;
    }
    const ac = new AbortController();
    abortRef.current = ac;
    const res = await fetch(`${API_URL}/api/v1/shell/${sid}/stream`, {
      headers: { Authorization: `Bearer ${token}` },
      credentials: "include",
      signal: ac.signal,
    });
    if (!res.ok || !res.body) {
      setError(`SSE failed (${res.status})`);
      return;
    }
    const reader = res.body.getReader();
    const decoder = new TextDecoder();
    let buf = "";
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      buf += decoder.decode(value, { stream: true });
      const parts = buf.split("\n\n");
      buf = parts.pop() ?? "";
      for (const block of parts) {
        const event = block.match(/^event: (.+)$/m)?.[1] ?? "message";
        const data = block
          .split("\n")
          .filter((l) => l.startsWith("data:"))
          .map((l) => l.slice(5).trimStart())
          .join("\n");
        if (event === "output") {
          setLines((prev) => prev + data);
        } else if (event === "closed") {
          setClosed(true);
          setLines((prev) => prev + `\n[session closed ${data}]\n`);
        }
      }
    }
  }

  async function send(e: FormEvent) {
    e.preventDefault();
    if (!sessionId) return;
    const payload = input.endsWith("\n") ? input : `${input}\n`;
    setInput("");
    try {
      await post(`/api/v1/shell/${sessionId}/input`, { data: payload });
    } catch (err) {
      setError(errorMessage(err));
    }
  }

  async function close() {
    if (!sessionId) return;
    abortRef.current?.abort();
    try {
      await del(`/api/v1/shell/${sessionId}/close`);
    } catch {
      /* already gone */
    }
    setClosed(true);
  }

  if (!isAdmin) {
    return <ErrorBanner message="Admin role required for remote shell." />;
  }

  return (
    <>
      <PageHeader
        title="Remote shell"
        subtitle={deviceId}
        actions={
          sessionId ? (
            <Button variant="danger" onClick={close} disabled={closed}>
              Close
            </Button>
          ) : (
            <Button onClick={openSession}>Open session</Button>
          )
        }
      />
      <ErrorBanner message={error} />
      <p className="mb-3 text-xs text-mute">
        Piped /bin/sh — not a PTY. Interactive programs and escape sequences are limited.
      </p>
      <pre className="h-[28rem] overflow-auto rounded-lg border border-line bg-ink p-3 font-mono text-xs leading-5">
        {lines || (sessionId ? "Waiting for output…" : "Open a session to start.")}
        <div ref={endRef} />
      </pre>
      <form className="mt-3 flex gap-2" onSubmit={send}>
        <Input
          value={input}
          onChange={(e) => setInput(e.target.value)}
          disabled={!sessionId || closed}
          placeholder="command"
          autoComplete="off"
          className="font-mono"
        />
        <Button type="submit" disabled={!sessionId || closed}>
          Send
        </Button>
      </form>
    </>
  );
}
