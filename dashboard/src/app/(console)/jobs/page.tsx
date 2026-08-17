"use client";

import { FormEvent, Suspense, useEffect, useState } from "react";
import Link from "next/link";
import { useSearchParams } from "next/navigation";
import { PageHeader } from "@/components/chrome";
import { Badge, Button, Empty, ErrorBanner, Field, Input, Modal, Select, Textarea } from "@/components/ui";
import { get, post } from "@/lib/api";
import { useAuth } from "@/lib/auth";
import { errorMessage, fmtTime } from "@/lib/format";
import type { Device, Job } from "@/lib/types";

export default function JobsPage() {
  return (
    <Suspense fallback={<p className="text-sm text-mute">Loading jobs…</p>}>
      <JobsInner />
    </Suspense>
  );
}

function JobsInner() {
  const { isAdmin } = useAuth();
  const search = useSearchParams();
  const preselect = search.get("device") ?? "";
  const [jobs, setJobs] = useState<Job[]>([]);
  const [devices, setDevices] = useState<Device[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [open, setOpen] = useState(false);

  async function load() {
    const q = preselect ? `?device_id=${preselect}` : "";
    const [j, d] = await Promise.all([get<Job[]>(`/api/v1/jobs${q}`), get<Device[]>("/api/v1/devices")]);
    setJobs(j);
    setDevices(d);
  }

  useEffect(() => {
    load().catch((err) => setError(errorMessage(err)));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [preselect]);

  return (
    <>
      <PageHeader
        title="Jobs"
        subtitle="Remote execution"
        actions={
          isAdmin ? (
            <Button onClick={() => setOpen(true)}>Dispatch job</Button>
          ) : undefined
        }
      />
      <ErrorBanner message={error} />
      {jobs.length === 0 ? (
        <Empty>No jobs yet.</Empty>
      ) : (
        <div className="overflow-x-auto rounded-lg border border-line">
          <table className="w-full text-left text-sm">
            <thead className="bg-raised text-xs uppercase text-mute">
              <tr>
                <th className="px-3 py-2 font-medium">Job</th>
                <th className="px-3 py-2 font-medium">Device</th>
                <th className="px-3 py-2 font-medium">Type</th>
                <th className="px-3 py-2 font-medium">Status</th>
                <th className="px-3 py-2 font-medium">Created</th>
              </tr>
            </thead>
            <tbody>
              {jobs.map((j) => (
                <tr key={j.id} className="border-t border-line hover:bg-raised/60">
                  <td className="px-3 py-2 font-mono text-xs">
                    <Link href={`/jobs/${j.id}`} className="hover:text-accent">
                      {j.id.slice(0, 8)}
                    </Link>
                  </td>
                  <td className="px-3 py-2">
                    <Link href={`/devices/${j.device_id}`} className="hover:text-accent">
                      {devices.find((d) => d.id === j.device_id)?.hostname ?? j.device_id.slice(0, 8)}
                    </Link>
                  </td>
                  <td className="px-3 py-2 text-mute">{payloadType(j.payload)}</td>
                  <td className="px-3 py-2">
                    <Badge tone={jobTone(j.status)}>{j.status}</Badge>
                  </td>
                  <td className="px-3 py-2 text-mute">{fmtTime(j.created_at)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      {open && (
        <CreateJobModal
          devices={devices}
          defaultDevice={preselect}
          onClose={() => setOpen(false)}
          onCreated={async () => {
            setOpen(false);
            await load();
          }}
        />
      )}
    </>
  );
}

function payloadType(payload: unknown): string {
  if (payload && typeof payload === "object" && "type" in payload) {
    return String((payload as { type: string }).type);
  }
  return "—";
}

function jobTone(status: string): "ok" | "bad" | "warn" | "mute" | "accent" {
  if (status === "completed" || status === "done") return "ok";
  if (status === "failed") return "bad";
  if (status === "cancelled") return "mute";
  return "accent";
}

function CreateJobModal({
  devices,
  defaultDevice,
  onClose,
  onCreated,
}: {
  devices: Device[];
  defaultDevice: string;
  onClose: () => void;
  onCreated: () => Promise<void>;
}) {
  const [deviceId, setDeviceId] = useState(defaultDevice || devices[0]?.id || "");
  const [kind, setKind] = useState("run_script");
  const [shell, setShell] = useState("bash");
  const [script, setScript] = useState("uname -a");
  const [delay, setDelay] = useState(60);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function submit(e: FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError(null);
    const payload =
      kind === "reboot"
        ? { type: "reboot", delay_seconds: Number(delay) }
        : kind === "collect_inventory"
          ? { type: "collect_inventory" }
          : { type: "run_script", shell, script };
    try {
      await post("/api/v1/jobs", { device_id: deviceId, payload });
      await onCreated();
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Modal title="Dispatch job" onClose={onClose}>
      <form className="space-y-3" onSubmit={submit}>
        <Field label="Device">
          <Select value={deviceId} onChange={(e) => setDeviceId(e.target.value)} required>
            {devices.map((d) => (
              <option key={d.id} value={d.id}>
                {d.hostname} ({d.status})
              </option>
            ))}
          </Select>
        </Field>
        <Field label="Type">
          <Select value={kind} onChange={(e) => setKind(e.target.value)}>
            <option value="run_script">Run script</option>
            <option value="reboot">Reboot</option>
            <option value="collect_inventory">Collect inventory (agent no-op; use Refresh inventory)</option>
          </Select>
        </Field>
        {kind === "run_script" && (
          <>
            <Field label="Shell">
              <Select value={shell} onChange={(e) => setShell(e.target.value)}>
                <option value="bash">bash</option>
                <option value="sh">sh</option>
                <option value="powershell">powershell</option>
                <option value="cmd">cmd</option>
              </Select>
            </Field>
            <Field label="Script">
              <Textarea rows={6} value={script} onChange={(e) => setScript(e.target.value)} />
            </Field>
          </>
        )}
        {kind === "reboot" && (
          <Field label="Delay (seconds)">
            <Input type="number" min={0} value={delay} onChange={(e) => setDelay(Number(e.target.value))} />
          </Field>
        )}
        <ErrorBanner message={error} />
        <div className="flex justify-end gap-2">
          <Button variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          <Button type="submit" disabled={busy || !deviceId}>
            {busy ? "Dispatching…" : "Dispatch"}
          </Button>
        </div>
      </form>
    </Modal>
  );
}
