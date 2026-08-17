"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import { PageHeader } from "@/components/chrome";
import { Badge, Card, Empty, ErrorBanner, StatusDot } from "@/components/ui";
import { get } from "@/lib/api";
import { errorMessage, fmtTime } from "@/lib/format";
import type { AlertEvent, Device, Job, ServerStatus } from "@/lib/types";

export default function OverviewPage() {
  const [status, setStatus] = useState<ServerStatus | null>(null);
  const [devices, setDevices] = useState<Device[]>([]);
  const [jobs, setJobs] = useState<Job[]>([]);
  const [alerts, setAlerts] = useState<AlertEvent[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    async function load() {
      try {
        const [s, d, j, a] = await Promise.all([
          get<ServerStatus>("/api/v1/settings/status"),
          get<Device[]>("/api/v1/devices"),
          get<Job[]>("/api/v1/jobs"),
          get<AlertEvent[]>("/api/v1/alerts/events?unresolved=true&limit=8"),
        ]);
        if (cancelled) return;
        setStatus(s);
        setDevices(d);
        setJobs(j.slice(0, 8));
        setAlerts(a);
        setError(null);
      } catch (err) {
        if (!cancelled) setError(errorMessage(err));
      }
    }
    load();
    const id = setInterval(load, 15000);
    return () => {
      cancelled = true;
      clearInterval(id);
    };
  }, []);

  const tiles = status
    ? [
        ["Online", `${status.online_devices} / ${status.total_devices}`],
        ["Connected", String(status.connected_agents)],
        ["Pending jobs", String(status.pending_jobs)],
        ["Policies", String(status.total_policies)],
      ]
    : [];

  return (
    <>
      <PageHeader
        title="Overview"
        subtitle={status ? `Server ${status.version}` : "Fleet snapshot"}
      />
      <ErrorBanner message={error} />
      <div className="mb-6 grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
        {tiles.map(([label, value]) => (
          <Card key={label}>
            <div className="text-xs uppercase tracking-wide text-mute">{label}</div>
            <div className="mt-1 text-2xl font-semibold">{value}</div>
          </Card>
        ))}
      </div>
      <div className="grid gap-4 lg:grid-cols-2">
        <Card>
          <h2 className="mb-3 text-sm font-medium">Devices</h2>
          {devices.length === 0 ? (
            <Empty>No devices enrolled yet.</Empty>
          ) : (
            <ul className="divide-y divide-line">
              {devices.slice(0, 8).map((d) => (
                <li key={d.id} className="flex items-center justify-between py-2 text-sm">
                  <Link href={`/devices/${d.id}`} className="hover:text-accent">
                    {d.hostname}
                  </Link>
                  <StatusDot status={d.status} />
                </li>
              ))}
            </ul>
          )}
        </Card>
        <Card>
          <h2 className="mb-3 text-sm font-medium">Recent jobs</h2>
          {jobs.length === 0 ? (
            <Empty>No jobs yet.</Empty>
          ) : (
            <ul className="divide-y divide-line">
              {jobs.map((j) => (
                <li key={j.id} className="flex items-center justify-between py-2 text-sm">
                  <Link href={`/jobs/${j.id}`} className="font-mono text-xs hover:text-accent">
                    {j.id.slice(0, 8)}
                  </Link>
                  <Badge tone={jobTone(j.status)}>{j.status}</Badge>
                </li>
              ))}
            </ul>
          )}
        </Card>
        <Card className="lg:col-span-2">
          <h2 className="mb-3 text-sm font-medium">Open alerts</h2>
          {alerts.length === 0 ? (
            <Empty>No unresolved alerts.</Empty>
          ) : (
            <ul className="divide-y divide-line">
              {alerts.map((a) => (
                <li key={a.id} className="flex items-center justify-between py-2 text-sm">
                  <span className="truncate pr-4">{a.message ?? "Alert"}</span>
                  <span className="shrink-0 text-xs text-mute">{fmtTime(a.triggered_at)}</span>
                </li>
              ))}
            </ul>
          )}
        </Card>
      </div>
    </>
  );
}

function jobTone(status: string): "ok" | "bad" | "warn" | "mute" | "accent" {
  if (status === "completed" || status === "done") return "ok";
  if (status === "failed") return "bad";
  if (status === "cancelled") return "mute";
  return "accent";
}
