"use client";

import { FormEvent, useEffect, useState } from "react";
import { PageHeader } from "@/components/chrome";
import {
  Badge,
  Button,
  Card,
  Empty,
  ErrorBanner,
  Field,
  Input,
  Modal,
  Select,
} from "@/components/ui";
import { del, get, post } from "@/lib/api";
import { useAuth } from "@/lib/auth";
import { errorMessage, fmtTime } from "@/lib/format";
import type { AlertEvent, AlertRule } from "@/lib/types";

export default function AlertsPage() {
  const { isAdmin } = useAuth();
  const [rules, setRules] = useState<AlertRule[]>([]);
  const [events, setEvents] = useState<AlertEvent[]>([]);
  const [unresolved, setUnresolved] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);

  async function load() {
    const q = unresolved ? "?unresolved=true" : "";
    const [r, e] = await Promise.all([
      get<AlertRule[]>("/api/v1/alerts/rules"),
      get<AlertEvent[]>(`/api/v1/alerts/events${q}`),
    ]);
    setRules(r);
    setEvents(e);
  }

  useEffect(() => {
    load().catch((err) => setError(errorMessage(err)));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [unresolved]);

  return (
    <>
      <PageHeader
        title="Alerts"
        subtitle="Threshold rules evaluated on every telemetry snapshot"
        actions={isAdmin ? <Button onClick={() => setCreating(true)}>New rule</Button> : undefined}
      />
      <ErrorBanner message={error} />
      <h2 className="mb-2 text-sm font-medium">Rules</h2>
      {rules.length === 0 ? (
        <Empty>No alert rules.</Empty>
      ) : (
        <div className="mb-8 space-y-2">
          {rules.map((r) => (
            <Card key={r.id} className="flex items-center justify-between">
              <div>
                <div className="font-medium">{r.name}</div>
                <div className="text-sm text-mute">
                  {r.metric} {r.operator} {r.threshold} · {r.severity}
                </div>
              </div>
              <div className="flex items-center gap-2">
                <Badge tone={r.enabled ? "ok" : "mute"}>{r.enabled ? "on" : "off"}</Badge>
                {isAdmin && (
                  <Button
                    variant="danger"
                    onClick={() =>
                      del(`/api/v1/alerts/rules/${r.id}`)
                        .then(load)
                        .catch((err) => setError(errorMessage(err)))
                    }
                  >
                    Delete
                  </Button>
                )}
              </div>
            </Card>
          ))}
        </div>
      )}
      <div className="mb-2 flex items-center justify-between">
        <h2 className="text-sm font-medium">Events</h2>
        <label className="flex items-center gap-2 text-sm text-mute">
          <input
            type="checkbox"
            checked={unresolved}
            onChange={(e) => setUnresolved(e.target.checked)}
          />
          Unresolved only
        </label>
      </div>
      {events.length === 0 ? (
        <Empty>No events.</Empty>
      ) : (
        <div className="overflow-x-auto rounded-lg border border-line">
          <table className="w-full text-left text-sm">
            <thead className="bg-raised text-xs uppercase text-mute">
              <tr>
                <th className="px-3 py-2 font-medium">When</th>
                <th className="px-3 py-2 font-medium">Severity</th>
                <th className="px-3 py-2 font-medium">Message</th>
                <th className="px-3 py-2 font-medium" />
              </tr>
            </thead>
            <tbody>
              {events.map((e) => (
                <tr key={e.id} className="border-t border-line">
                  <td className="px-3 py-2 text-mute">{fmtTime(e.triggered_at)}</td>
                  <td className="px-3 py-2">
                    <Badge tone={e.severity === "critical" ? "bad" : "warn"}>
                      {e.severity ?? "info"}
                    </Badge>
                  </td>
                  <td className="px-3 py-2">{e.message}</td>
                  <td className="px-3 py-2 text-right">
                    {isAdmin && !e.resolved_at && (
                      <Button
                        variant="outline"
                        onClick={() =>
                          post(`/api/v1/alerts/events/${e.id}/resolve`)
                            .then(load)
                            .catch((err) => setError(errorMessage(err)))
                        }
                      >
                        Resolve
                      </Button>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      {creating && (
        <CreateRuleModal
          onClose={() => setCreating(false)}
          onCreated={async () => {
            setCreating(false);
            await load();
          }}
        />
      )}
    </>
  );
}

function CreateRuleModal({
  onClose,
  onCreated,
}: {
  onClose: () => void;
  onCreated: () => Promise<void>;
}) {
  const [name, setName] = useState("High CPU");
  const [metric, setMetric] = useState("cpu_pct");
  const [operator, setOperator] = useState(">");
  const [threshold, setThreshold] = useState(90);
  const [severity, setSeverity] = useState("warning");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function submit(e: FormEvent) {
    e.preventDefault();
    setBusy(true);
    try {
      await post("/api/v1/alerts/rules", { name, metric, operator, threshold, severity });
      await onCreated();
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Modal title="New alert rule" onClose={onClose}>
      <form className="space-y-3" onSubmit={submit}>
        <Field label="Name">
          <Input value={name} onChange={(e) => setName(e.target.value)} required />
        </Field>
        <Field label="Metric">
          <Select value={metric} onChange={(e) => setMetric(e.target.value)}>
            <option value="cpu_pct">CPU %</option>
            <option value="ram_pct">RAM %</option>
            <option value="disk_pct">Disk %</option>
          </Select>
        </Field>
        <div className="grid grid-cols-2 gap-2">
          <Field label="Operator">
            <Select value={operator} onChange={(e) => setOperator(e.target.value)}>
              <option value=">">&gt;</option>
              <option value=">=">&gt;=</option>
              <option value="<">&lt;</option>
              <option value="<=">&lt;=</option>
              <option value="==">==</option>
            </Select>
          </Field>
          <Field label="Threshold">
            <Input
              type="number"
              value={threshold}
              onChange={(e) => setThreshold(Number(e.target.value))}
            />
          </Field>
        </div>
        <Field label="Severity">
          <Select value={severity} onChange={(e) => setSeverity(e.target.value)}>
            <option value="info">info</option>
            <option value="warning">warning</option>
            <option value="critical">critical</option>
          </Select>
        </Field>
        <ErrorBanner message={error} />
        <div className="flex justify-end gap-2">
          <Button variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          <Button type="submit" disabled={busy}>
            Create
          </Button>
        </div>
      </form>
    </Modal>
  );
}
