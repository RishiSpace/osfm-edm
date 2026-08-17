"use client";

import { FormEvent, useEffect, useState } from "react";
import { PageHeader } from "@/components/chrome";
import { Button, Card, Empty, ErrorBanner, Field, Input, Modal, Select } from "@/components/ui";
import { del, get, post } from "@/lib/api";
import { useAuth } from "@/lib/auth";
import { errorMessage } from "@/lib/format";
import type { Device, Group, GroupMember } from "@/lib/types";

export default function GroupsPage() {
  const { isAdmin } = useAuth();
  const [groups, setGroups] = useState<Group[]>([]);
  const [devices, setDevices] = useState<Device[]>([]);
  const [members, setMembers] = useState<Record<string, GroupMember[]>>({});
  const [error, setError] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);

  async function load() {
    const [g, d] = await Promise.all([get<Group[]>("/api/v1/groups"), get<Device[]>("/api/v1/devices")]);
    setGroups(g);
    setDevices(d);
    const entries = await Promise.all(
      g.map(async (group) => {
        const list = await get<GroupMember[]>(`/api/v1/groups/${group.id}/members`);
        return [group.id, list] as const;
      }),
    );
    setMembers(Object.fromEntries(entries));
  }

  useEffect(() => {
    load().catch((err) => setError(errorMessage(err)));
  }, []);

  return (
    <>
      <PageHeader
        title="Groups"
        subtitle="Assign policies to a set of devices"
        actions={isAdmin ? <Button onClick={() => setCreating(true)}>New group</Button> : undefined}
      />
      <ErrorBanner message={error} />
      {groups.length === 0 ? (
        <Empty>No groups.</Empty>
      ) : (
        <div className="space-y-3">
          {groups.map((g) => (
            <Card key={g.id}>
              <div className="flex items-start justify-between">
                <div>
                  <h2 className="font-medium">{g.name}</h2>
                  {g.description && <p className="text-sm text-mute">{g.description}</p>}
                </div>
                {isAdmin && (
                  <Button
                    variant="danger"
                    onClick={() => {
                      if (confirm(`Delete group “${g.name}”?`)) {
                        del(`/api/v1/groups/${g.id}`)
                          .then(load)
                          .catch((err) => setError(errorMessage(err)));
                      }
                    }}
                  >
                    Delete
                  </Button>
                )}
              </div>
              <ul className="mt-3 space-y-1 text-sm">
                {(members[g.id] ?? []).map((m) => (
                  <li key={m.device_id} className="flex items-center justify-between">
                    <span>
                      {m.hostname}{" "}
                      <span className="text-mute">
                        · {m.os} · {m.status}
                      </span>
                    </span>
                    {isAdmin && (
                      <Button
                        variant="ghost"
                        onClick={() =>
                          del(`/api/v1/groups/${g.id}/members/${m.device_id}`)
                            .then(load)
                            .catch((err) => setError(errorMessage(err)))
                        }
                      >
                        Remove
                      </Button>
                    )}
                  </li>
                ))}
              </ul>
              {isAdmin && (
                <AddMember
                  groupId={g.id}
                  devices={devices}
                  existing={new Set((members[g.id] ?? []).map((m) => m.device_id))}
                  onAdded={load}
                  onError={setError}
                />
              )}
            </Card>
          ))}
        </div>
      )}
      {creating && (
        <CreateGroupModal
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

function AddMember({
  groupId,
  devices,
  existing,
  onAdded,
  onError,
}: {
  groupId: string;
  devices: Device[];
  existing: Set<string>;
  onAdded: () => Promise<void>;
  onError: (m: string | null) => void;
}) {
  const [deviceId, setDeviceId] = useState("");
  const available = devices.filter((d) => !existing.has(d.id));
  return (
    <div className="mt-3 flex gap-2 border-t border-line pt-3">
      <Select value={deviceId} onChange={(e) => setDeviceId(e.target.value)} className="max-w-xs">
        <option value="">Add device…</option>
        {available.map((d) => (
          <option key={d.id} value={d.id}>
            {d.hostname}
          </option>
        ))}
      </Select>
      <Button
        variant="outline"
        disabled={!deviceId}
        onClick={() => {
          onError(null);
          post(`/api/v1/groups/${groupId}/members`, { device_id: deviceId })
            .then(onAdded)
            .catch((err) => onError(errorMessage(err)));
        }}
      >
        Add
      </Button>
    </div>
  );
}

function CreateGroupModal({
  onClose,
  onCreated,
}: {
  onClose: () => void;
  onCreated: () => Promise<void>;
}) {
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function submit(e: FormEvent) {
    e.preventDefault();
    setBusy(true);
    try {
      await post("/api/v1/groups", { name, description: description || null });
      await onCreated();
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Modal title="New group" onClose={onClose}>
      <form className="space-y-3" onSubmit={submit}>
        <Field label="Name">
          <Input value={name} onChange={(e) => setName(e.target.value)} required />
        </Field>
        <Field label="Description">
          <Input value={description} onChange={(e) => setDescription(e.target.value)} />
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
