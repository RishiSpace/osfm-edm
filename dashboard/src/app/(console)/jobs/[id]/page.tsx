"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import { useParams } from "next/navigation";
import { PageHeader } from "@/components/chrome";
import { Badge, Button, Card, Empty, ErrorBanner } from "@/components/ui";
import { get, post } from "@/lib/api";
import { useAuth } from "@/lib/auth";
import { errorMessage, fmtTime } from "@/lib/format";
import type { Job, JobLog } from "@/lib/types";

type JobDetail = Job & { logs?: JobLog[] };

export default function JobDetailPage() {
  const { id } = useParams<{ id: string }>();
  const { isAdmin } = useAuth();
  const [job, setJob] = useState<JobDetail | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function load() {
    const data = await get<JobDetail>(`/api/v1/jobs/${id}`);
    setJob(data);
  }

  useEffect(() => {
    load().catch((err) => setError(errorMessage(err)));
    const t = setInterval(() => {
      load().catch(() => undefined);
    }, 3000);
    return () => clearInterval(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [id]);

  const running = job && !["completed", "done", "failed", "cancelled"].includes(job.status);

  return (
    <>
      <PageHeader
        title="Job"
        subtitle={job?.id}
        actions={
          isAdmin && running ? (
            <Button
              variant="danger"
              onClick={() =>
                post(`/api/v1/jobs/${id}/cancel`)
                  .then(load)
                  .catch((err) => setError(errorMessage(err)))
              }
            >
              Cancel
            </Button>
          ) : undefined
        }
      />
      <ErrorBanner message={error} />
      {!job ? (
        <Empty>Loading…</Empty>
      ) : (
        <>
          <div className="mb-4 flex flex-wrap gap-3 text-sm text-mute">
            <Badge
              tone={
                job.status === "failed"
                  ? "bad"
                  : job.status === "completed" || job.status === "done"
                    ? "ok"
                    : "accent"
              }
            >
              {job.status}
            </Badge>
            <Link href={`/devices/${job.device_id}`} className="hover:text-accent">
              Device {job.device_id.slice(0, 8)}
            </Link>
            <span>Created {fmtTime(job.created_at)}</span>
            <span>Finished {fmtTime(job.finished_at)}</span>
            {job.exit_code != null && <span>Exit {job.exit_code}</span>}
          </div>
          <Card className="mb-4">
            <h2 className="mb-2 text-xs uppercase tracking-wide text-mute">Payload</h2>
            <pre className="overflow-x-auto font-mono text-xs text-mute">
              {JSON.stringify(job.payload, null, 2)}
            </pre>
          </Card>
          <Card>
            <h2 className="mb-2 text-xs uppercase tracking-wide text-mute">Logs</h2>
            {!job.logs?.length ? (
              <Empty>No log lines yet.</Empty>
            ) : (
              <pre className="max-h-[28rem] overflow-auto font-mono text-xs leading-5">
                {job.logs.map((l, i) => (
                  <div key={`${l.time}-${i}`} className={l.stream === "stderr" ? "text-bad" : ""}>
                    {l.line}
                  </div>
                ))}
              </pre>
            )}
          </Card>
        </>
      )}
    </>
  );
}
