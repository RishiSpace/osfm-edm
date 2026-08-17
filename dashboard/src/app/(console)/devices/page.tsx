"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import { PageHeader } from "@/components/chrome";
import { Empty, ErrorBanner, StatusDot } from "@/components/ui";
import { get } from "@/lib/api";
import { errorMessage, fmtTime } from "@/lib/format";
import type { Device } from "@/lib/types";

export default function DevicesPage() {
  const [devices, setDevices] = useState<Device[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    get<Device[]>("/api/v1/devices")
      .then(setDevices)
      .catch((err) => setError(errorMessage(err)));
  }, []);

  return (
    <>
      <PageHeader title="Devices" subtitle="Enrolled endpoints" />
      <ErrorBanner message={error} />
      {devices.length === 0 && !error ? (
        <Empty>No devices. Generate an enrollment token in Settings.</Empty>
      ) : (
        <div className="overflow-x-auto rounded-lg border border-line">
          <table className="w-full text-left text-sm">
            <thead className="bg-raised text-xs uppercase text-mute">
              <tr>
                <th className="px-3 py-2 font-medium">Host</th>
                <th className="px-3 py-2 font-medium">OS</th>
                <th className="px-3 py-2 font-medium">Status</th>
                <th className="px-3 py-2 font-medium">Agent</th>
                <th className="px-3 py-2 font-medium">Last seen</th>
              </tr>
            </thead>
            <tbody>
              {devices.map((d) => (
                <tr key={d.id} className="border-t border-line hover:bg-raised/60">
                  <td className="px-3 py-2">
                    <Link href={`/devices/${d.id}`} className="font-medium hover:text-accent">
                      {d.hostname}
                    </Link>
                  </td>
                  <td className="px-3 py-2 text-mute">
                    {d.os}
                    {d.os_version ? ` ${d.os_version}` : ""}
                  </td>
                  <td className="px-3 py-2">
                    <StatusDot status={d.status} />
                  </td>
                  <td className="px-3 py-2 font-mono text-xs text-mute">
                    {d.agent_version ?? "—"}
                  </td>
                  <td className="px-3 py-2 text-mute">{fmtTime(d.last_seen)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </>
  );
}
