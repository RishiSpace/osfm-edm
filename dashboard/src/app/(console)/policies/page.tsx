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
  Textarea,
} from "@/components/ui";
import { del, get, patch, post } from "@/lib/api";
import { useAuth } from "@/lib/auth";
import { errorMessage } from "@/lib/format";
import type { Device, Group, Policy } from "@/lib/types";

export default function PoliciesPage() {
  const { isAdmin } = useAuth();
  const [policies, setPolicies] = useState<Policy[]>([]);
  const [devices, setDevices] = useState<Device[]>([]);
  const [groups, setGroups] = useState<Group[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);

  async function load() {
    const [p, d, g] = await Promise.all([
      get<Policy[]>("/api/v1/policies"),
      get<Device[]>("/api/v1/devices"),
      get<Group[]>("/api/v1/groups"),
    ]);
    setPolicies(p);
    setDevices(d);
    setGroups(g);
  }

  useEffect(() => {
    load().catch((err) => setError(errorMessage(err)));
  }, []);

  return (
    <>
      <PageHeader
        title="Policies"
        subtitle="Compliance rules pushed to assigned devices"
        actions={
          isAdmin ? <Button onClick={() => setCreating(true)}>New policy</Button> : undefined
        }
      />
      <ErrorBanner message={error} />
      {policies.length === 0 ? (
        <Empty>No policies defined.</Empty>
      ) : (
        <div className="space-y-3">
          {policies.map((p) => (
            <Card key={p.id}>
              <div className="flex flex-wrap items-start justify-between gap-3">
                <div>
                  <div className="flex items-center gap-2">
                    <h2 className="font-medium">{p.name}</h2>
                    <Badge tone={p.enabled ? "ok" : "mute"}>{p.enabled ? "enabled" : "disabled"}</Badge>
                    <span className="text-xs text-mute">v{p.version}</span>
                  </div>
                  {p.description && <p className="mt-1 text-sm text-mute">{p.description}</p>}
                </div>
                {isAdmin && (
                  <div className="flex gap-2">
                    <Button
                      variant="outline"
                      onClick={() =>
                        patch(`/api/v1/policies/${p.id}`, { enabled: !p.enabled })
                          .then(load)
                          .catch((err) => setError(errorMessage(err)))
                      }
                    >
                      {p.enabled ? "Disable" : "Enable"}
                    </Button>
                    <Button
                      variant="danger"
                      onClick={() => {
                        if (confirm(`Delete policy “${p.name}”?`)) {
                          del(`/api/v1/policies/${p.id}`)
                            .then(load)
                            .catch((err) => setError(errorMessage(err)));
                        }
                      }}
                    >
                      Delete
                    </Button>
                  </div>
                )}
              </div>
              <pre className="mt-3 overflow-x-auto font-mono text-xs text-mute">
                {JSON.stringify(p.rules, null, 2)}
              </pre>
              {isAdmin && (
                <AssignRow
                  policyId={p.id}
                  devices={devices}
                  groups={groups}
                  onError={setError}
                />
              )}
            </Card>
          ))}
        </div>
      )}
      {creating && (
        <CreatePolicyModal
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

function AssignRow({
  policyId,
  devices,
  groups,
  onError,
}: {
  policyId: string;
  devices: Device[];
  groups: Group[];
  onError: (m: string | null) => void;
}) {
  const [deviceId, setDeviceId] = useState("");
  const [groupId, setGroupId] = useState("");

  async function assign() {
    onError(null);
    try {
      await post(`/api/v1/policies/${policyId}/assign`, {
        device_id: deviceId || null,
        group_id: groupId || null,
      });
    } catch (err) {
      onError(errorMessage(err));
    }
  }

  return (
    <div className="mt-3 flex flex-wrap items-end gap-2 border-t border-line pt-3">
      <Field label="Assign device">
        <Select value={deviceId} onChange={(e) => setDeviceId(e.target.value)}>
          <option value="">—</option>
          {devices.map((d) => (
            <option key={d.id} value={d.id}>
              {d.hostname}
            </option>
          ))}
        </Select>
      </Field>
      <Field label="Assign group">
        <Select value={groupId} onChange={(e) => setGroupId(e.target.value)}>
          <option value="">—</option>
          {groups.map((g) => (
            <option key={g.id} value={g.id}>
              {g.name}
            </option>
          ))}
        </Select>
      </Field>
      <Button variant="outline" onClick={assign} disabled={!deviceId && !groupId}>
        Assign
      </Button>
    </div>
  );
}

function CreatePolicyModal({
  onClose,
  onCreated,
}: {
  onClose: () => void;
  onCreated: () => Promise<void>;
}) {
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [firewall, setFirewall] = useState(true);
  const [blockUsb, setBlockUsb] = useState(false);
  const [screenLock, setScreenLock] = useState(5);
  const [updates, setUpdates] = useState("security_only");
  const [deny, setDeny] = useState("");
  const [raw, setRaw] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  function buildRules(): unknown[] {
    if (raw.trim()) return JSON.parse(raw) as unknown[];
    const rules: unknown[] = [
      { type: "firewall", enabled: firewall },
      { type: "usb_storage", allow: !blockUsb },
    ];
    if (screenLock > 0) {
      rules.push({
        type: "screen_lock",
        timeout_minutes: screenLock,
        require_password: true,
      });
    }
    rules.push({ type: "os_update", auto_install: updates });
    const list = deny
      .split(",")
      .map((s) => s.trim())
      .filter(Boolean);
    if (list.length) rules.push({ type: "process_blacklist", deny: list });
    return rules;
  }

  async function submit(e: FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      const rules = buildRules();
      await post("/api/v1/policies", { name, description: description || null, rules });
      await onCreated();
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Modal title="New policy" onClose={onClose}>
      <form className="space-y-3" onSubmit={submit}>
        <Field label="Name">
          <Input value={name} onChange={(e) => setName(e.target.value)} required />
        </Field>
        <Field label="Description">
          <Input value={description} onChange={(e) => setDescription(e.target.value)} />
        </Field>
        <label className="flex items-center gap-2 text-sm">
          <input type="checkbox" checked={firewall} onChange={(e) => setFirewall(e.target.checked)} />
          Require firewall
        </label>
        <label className="flex items-center gap-2 text-sm">
          <input type="checkbox" checked={blockUsb} onChange={(e) => setBlockUsb(e.target.checked)} />
          Block USB storage
        </label>
        <Field label="Screen lock (minutes, 0 = skip)">
          <Input
            type="number"
            min={0}
            value={screenLock}
            onChange={(e) => setScreenLock(Number(e.target.value))}
          />
        </Field>
        <Field label="Auto updates">
          <Select value={updates} onChange={(e) => setUpdates(e.target.value)}>
            <option value="disabled">Disabled</option>
            <option value="security_only">Security only</option>
            <option value="all">All</option>
          </Select>
        </Field>
        <Field label="Process blacklist (comma-separated)">
          <Input value={deny} onChange={(e) => setDeny(e.target.value)} placeholder="minerd, xmrig" />
        </Field>
        <Field label="Or paste rules JSON array (overrides form)">
          <Textarea
            rows={4}
            value={raw}
            onChange={(e) => setRaw(e.target.value)}
            placeholder='[{"type":"firewall","enabled":true}]'
          />
        </Field>
        <ErrorBanner message={error} />
        <div className="flex justify-end gap-2">
          <Button variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          <Button type="submit" disabled={busy}>
            {busy ? "Creating…" : "Create"}
          </Button>
        </div>
      </form>
    </Modal>
  );
}
