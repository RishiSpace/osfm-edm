"use client";

import { useEffect, useMemo, useState } from "react";
import Link from "next/link";
import { useParams } from "next/navigation";
import {
  CartesianGrid,
  Line,
  LineChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import { PageHeader } from "@/components/chrome";
import { Button, Card, Empty, ErrorBanner, StatusDot } from "@/components/ui";
import { del, get, post } from "@/lib/api";
import { useAuth } from "@/lib/auth";
import { errorMessage, fmtBytesMb, fmtTime, fmtUptime, pct } from "@/lib/format";
import type { Device, Metric, PatchItem, SoftwareItem } from "@/lib/types";

type Tab = "telemetry" | "software" | "patches";

export default function DeviceDetailPage() {
  const { id } = useParams<{ id: string }>();
  const { isAdmin } = useAuth();
  const [device, setDevice] = useState<Device | null>(null);
  const [metrics, setMetrics] = useState<Metric[]>([]);
  const [software, setSoftware] = useState<SoftwareItem[]>([]);
  const [patches, setPatches] = useState<PatchItem[]>([]);
  const [tab, setTab] = useState<Tab>("telemetry");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);

  async function load() {
    const [d, m, s, p] = await Promise.all([
      get<Device>(`/api/v1/devices/${id}`),
      get<Metric[]>(`/api/v1/devices/${id}/telemetry`),
      get<SoftwareItem[]>(`/api/v1/software/device/${id}`),
      get<{ patches: PatchItem[] }>(`/api/v1/patches/device/${id}`),
    ]);
    setDevice(d);
    setMetrics(m);
    setSoftware(s);
    setPatches(p.patches);
  }

  useEffect(() => {
    load()
      .then(() => setError(null))
      .catch((err) => setError(errorMessage(err)));
    const t = setInterval(() => {
      load().catch(() => undefined);
    }, 20000);
    return () => clearInterval(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [id]);

  const chart = useMemo(
    () =>
      metrics.map((m) => ({
        t: new Date(m.time).toLocaleTimeString(),
        cpu: m.cpu_pct ?? 0,
        ram: pct(m.ram_used_mb, m.ram_total_mb) ?? 0,
        disk: pct(m.disk_used_gb, m.disk_total_gb) ?? 0,
      })),
    [metrics],
  );
  const latest = metrics[metrics.length - 1];

  async function run(label: string, fn: () => Promise<unknown>) {
    setBusy(label);
    setError(null);
    try {
      await fn();
      await load();
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setBusy(null);
    }
  }

  if (!device && !error) {
    return <p className="text-sm text-mute">Loading device…</p>;
  }

  return (
    <>
      <PageHeader
        title={device?.hostname ?? "Device"}
        subtitle={device ? `${device.os} · ${device.arch ?? "unknown arch"}` : undefined}
        actions={
          isAdmin && device ? (
            <div className="flex flex-wrap gap-2">
              <Link href={`/shell/${device.id}`}>
                <Button>Shell</Button>
              </Link>
              <Link href={`/jobs?device=${device.id}`}>
                <Button variant="outline">Jobs</Button>
              </Link>
              <Button
                variant="outline"
                disabled={busy !== null}
                onClick={() =>
                  run("inv", () => post(`/api/v1/devices/${device.id}/request-inventory`))
                }
              >
                Refresh inventory
              </Button>
              <Button
                variant="outline"
                disabled={busy !== null}
                onClick={() =>
                  run("tel", () => post(`/api/v1/devices/${device.id}/request-telemetry`))
                }
              >
                Snapshot
              </Button>
              <Button
                variant="danger"
                disabled={busy !== null}
                onClick={() => {
                  if (confirm("Revoke this device certificate? Re-enroll to reconnect after token wipe.")) {
                    run("rev", () => del(`/api/v1/devices/${device.id}`));
                  }
                }}
              >
                Revoke
              </Button>
            </div>
          ) : null
        }
      />
      <ErrorBanner message={error} />
      {device && (
        <div className="mb-4 flex flex-wrap gap-4 text-sm text-mute">
          <StatusDot status={device.status} />
          <span>Last seen {fmtTime(device.last_seen)}</span>
          <span>Enrolled {fmtTime(device.enrolled_at)}</span>
          <span className="font-mono text-xs">{device.id}</span>
        </div>
      )}
      {latest && (
        <div className="mb-6 grid gap-3 sm:grid-cols-4">
          <Stat label="CPU" value={`${(latest.cpu_pct ?? 0).toFixed(1)}%`} />
          <Stat
            label="RAM"
            value={`${fmtBytesMb(latest.ram_used_mb)} / ${fmtBytesMb(latest.ram_total_mb)}`}
          />
          <Stat
            label="Disk"
            value={`${(latest.disk_used_gb ?? 0).toFixed(1)} / ${(latest.disk_total_gb ?? 0).toFixed(1)} GB`}
          />
          <Stat label="Uptime" value={fmtUptime(latest.uptime_secs)} />
        </div>
      )}
      <div className="mb-3 flex gap-2">
        {(["telemetry", "software", "patches"] as Tab[]).map((t) => (
          <button
            key={t}
            type="button"
            onClick={() => setTab(t)}
            className={`rounded-md px-3 py-1.5 text-sm capitalize ${
              tab === t ? "bg-raised text-accent" : "text-mute hover:text-white"
            }`}
          >
            {t}
          </button>
        ))}
      </div>
      {tab === "telemetry" && (
        <Card className="h-80">
          {chart.length === 0 ? (
            <Empty>No telemetry in the last 24 hours.</Empty>
          ) : (
            <ResponsiveContainer width="100%" height="100%">
              <LineChart data={chart}>
                <CartesianGrid stroke="#26262c" strokeDasharray="3 3" />
                <XAxis dataKey="t" stroke="#8b8b96" fontSize={11} minTickGap={24} />
                <YAxis stroke="#8b8b96" fontSize={11} domain={[0, 100]} />
                <Tooltip
                  contentStyle={{ background: "#0e0e10", border: "1px solid #26262c" }}
                />
                <Line type="monotone" dataKey="cpu" stroke="#15dae3" dot={false} name="CPU %" />
                <Line type="monotone" dataKey="ram" stroke="#3dd68c" dot={false} name="RAM %" />
                <Line type="monotone" dataKey="disk" stroke="#e6b450" dot={false} name="Disk %" />
              </LineChart>
            </ResponsiveContainer>
          )}
        </Card>
      )}
      {tab === "software" && (
        <ItemTable
          rows={software.map((s) => [s.name, s.version ?? "—", s.publisher ?? "—"])}
          empty="No inventory yet. Request a refresh while the agent is online."
        />
      )}
      {tab === "patches" && (
        <ItemTable
          rows={patches.map((p) => [p.title ?? p.patch_id, p.status, p.severity ?? "—"])}
          empty="No pending patches reported."
        />
      )}
    </>
  );
}

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <Card>
      <div className="text-xs uppercase tracking-wide text-mute">{label}</div>
      <div className="mt-1 text-sm font-medium">{value}</div>
    </Card>
  );
}

function ItemTable({ rows, empty }: { rows: string[][]; empty: string }) {
  if (rows.length === 0) return <Empty>{empty}</Empty>;
  return (
    <div className="overflow-x-auto rounded-lg border border-line">
      <table className="w-full text-left text-sm">
        <tbody>
          {rows.map((cols, i) => (
            <tr key={`${cols[0]}-${i}`} className="border-t border-line first:border-0">
              {cols.map((c) => (
                <td key={c} className="px-3 py-2">
                  {c === cols[0] ? c : <span className="text-mute">{c}</span>}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
