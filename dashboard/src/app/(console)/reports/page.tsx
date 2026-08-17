"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import { PageHeader } from "@/components/chrome";
import { Badge, Card, Empty, ErrorBanner } from "@/components/ui";
import { get } from "@/lib/api";
import { errorMessage, fmtTime } from "@/lib/format";
import type { ComplianceFleet } from "@/lib/types";

export default function ReportsPage() {
  const [data, setData] = useState<ComplianceFleet | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    get<ComplianceFleet>("/api/v1/reports/compliance")
      .then(setData)
      .catch((err) => setError(errorMessage(err)));
  }, []);

  return (
    <>
      <PageHeader title="Compliance" subtitle="Latest per-device policy evaluations" />
      <ErrorBanner message={error} />
      {data && (
        <div className="mb-6 grid gap-3 sm:grid-cols-3">
          <Card>
            <div className="text-xs uppercase text-mute">Rate</div>
            <div className="text-2xl font-semibold">{data.compliance_rate.toFixed(1)}%</div>
          </Card>
          <Card>
            <div className="text-xs uppercase text-mute">Compliant</div>
            <div className="text-2xl font-semibold">{data.compliant}</div>
          </Card>
          <Card>
            <div className="text-xs uppercase text-mute">Violations</div>
            <div className="text-2xl font-semibold">{data.non_compliant}</div>
          </Card>
        </div>
      )}
      {!data?.recent_violations.length ? (
        <Empty>No recent violations.</Empty>
      ) : (
        <div className="overflow-x-auto rounded-lg border border-line">
          <table className="w-full text-left text-sm">
            <thead className="bg-raised text-xs uppercase text-mute">
              <tr>
                <th className="px-3 py-2 font-medium">Device</th>
                <th className="px-3 py-2 font-medium">Policy</th>
                <th className="px-3 py-2 font-medium">Status</th>
                <th className="px-3 py-2 font-medium">When</th>
              </tr>
            </thead>
            <tbody>
              {data.recent_violations.map((v) => (
                <tr key={v.id} className="border-t border-line">
                  <td className="px-3 py-2">
                    <Link href={`/devices/${v.device_id}`} className="hover:text-accent">
                      {v.device_id.slice(0, 8)}
                    </Link>
                  </td>
                  <td className="px-3 py-2 font-mono text-xs">{v.policy_id.slice(0, 8)}</td>
                  <td className="px-3 py-2">
                    <Badge tone={v.compliant ? "ok" : "bad"}>
                      {v.compliant ? "ok" : "violation"}
                    </Badge>
                  </td>
                  <td className="px-3 py-2 text-mute">{fmtTime(v.reported_at)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </>
  );
}
