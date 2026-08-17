export type Role = "admin" | "viewer";

export type User = {
  id: string;
  username: string;
  role: Role;
  totp_enabled: boolean;
  created_at: string | null;
  last_login: string | null;
};

export type Device = {
  id: string;
  hostname: string;
  os: string;
  os_version: string | null;
  arch: string | null;
  ip_address: string | null;
  agent_version: string | null;
  enrolled_at: string | null;
  last_seen: string | null;
  status: "online" | "offline" | "stale" | string;
};

export type Metric = {
  device_id: string;
  time: string;
  cpu_pct: number | null;
  ram_used_mb: number | null;
  ram_total_mb: number | null;
  disk_used_gb: number | null;
  disk_total_gb: number | null;
  uptime_secs: number | null;
};

export type Job = {
  id: string;
  device_id: string;
  payload: unknown;
  status: string;
  exit_code: number | null;
  created_by: string | null;
  created_at: string | null;
  finished_at: string | null;
  logs?: JobLog[];
};

export type JobLog = {
  time: string | null;
  line: string;
  stream: string;
};

export type Policy = {
  id: string;
  name: string;
  description: string | null;
  rules: unknown;
  version: number;
  enabled: boolean;
  created_at: string | null;
  updated_at: string | null;
};

export type Group = {
  id: string;
  name: string;
  description: string | null;
  created_at: string | null;
};

export type GroupMember = {
  device_id: string;
  hostname: string;
  os: string;
  status: string;
};

export type AlertRule = {
  id: string;
  name: string;
  metric: string | null;
  operator: string | null;
  threshold: number | null;
  severity: string | null;
  channels: unknown;
  enabled: boolean;
  created_at: string | null;
};

export type AlertEvent = {
  id: string;
  rule_id: string;
  device_id: string | null;
  severity: string | null;
  message: string | null;
  triggered_at: string | null;
  resolved_at: string | null;
};

export type SoftwareItem = {
  id: string;
  device_id: string;
  name: string;
  version: string | null;
  publisher: string | null;
  install_date: string | null;
};

export type PatchItem = {
  patch_id: string;
  title: string | null;
  severity: string | null;
  status: string;
  detected_at: string | null;
};

export type ServerStatus = {
  total_devices: number;
  online_devices: number;
  connected_agents: number;
  total_users: number;
  total_policies: number;
  pending_jobs: number;
  version: string;
};

export type ComplianceFleet = {
  total_evaluations: number;
  compliant: number;
  non_compliant: number;
  compliance_rate: number;
  recent_violations: Array<{
    id: string;
    device_id: string;
    policy_id: string;
    compliant: boolean;
    reported_at: string | null;
  }>;
};

export type Envelope<T> = {
  data: T | null;
  error: { code: string; message: string } | null;
};
