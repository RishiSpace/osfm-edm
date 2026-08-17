"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import { PageHeader } from "@/components/chrome";
import { Card, Empty, ErrorBanner } from "@/components/ui";
import { get } from "@/lib/api";
import { errorMessage } from "@/lib/format";
import type { Device } from "@/lib/types";

type PatchSummary = {
  total_devices: number;
  devices_with_pending_patches: number;
  pending_by_severity: Array<{ severity: string | null; count: number }>;
};

export default function InventoryPage() {
  const [devices, setDevices] = useState<Device[]>([]);
  const [summary, setSummary] = useState<PatchSummary | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    Promise.all([get<Device[]>("/api/v1/devices"), get<PatchSummary>("/api/v1/patches/summary")])
      .then(([d, s]) => {
        setDevices(d);
        setSummary(s);
      })
      .catch((err) => setError(errorMessage(err)));
  }, []);

  return (
    <>
      <PageHeader title="Inventory" subtitle="Software and pending patches by device" />
      <ErrorBanner message={error} />
      {summary && (
        <div className="mb-6 grid gap-3 sm:grid-cols-3">
          <Card>
            <div className="text-xs uppercase text-mute">Devices</div>
            <div className="text-2xl font-semibold">{summary.total_devices}</div>
          </Card>
          <Card>
            <div className="text-xs uppercase text-mute">With pending patches</div>
            <div className="text-2xl font-semibold">{summary.devices_with_pending_patches}</div>
          </Card>
          <Card>
            <div className="text-xs uppercase text-mute">By severity</div>
            <div className="mt-1 space-y-1 text-sm">
              {summary.pending_by_severity.length === 0
                ? "None"
                : summary.pending_by_severity.map((s) => (
                    <div key={s.severity ?? "unknown"}>
                      {s.severity ?? "unknown"}: {s.count}
                    </div>
                  ))}
            </div>
          </Card>
        </div>
      )}
      {devices.length === 0 ? (
        <Empty>No devices.</Empty>
      ) : (
        <ul className="divide-y divide-line rounded-lg border border-line">
          {devices.map((d) => (
            <li key={d.id} className="flex items-center justify-between px-3 py-2 text-sm">
              <Link href={`/devices/${d.id}`} className="hover:text-accent">
                {d.hostname}
              </Link>
              <span className="text-mute">{d.os}</span>
            </li>
          ))}
        </ul>
      )}
    </>
  );
}
