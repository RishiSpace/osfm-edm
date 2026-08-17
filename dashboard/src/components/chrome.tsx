"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import {
  Activity,
  Bell,
  Box,
  FileText,
  LayoutDashboard,
  Monitor,
  ScrollText,
  Settings,
  Shield,
  Users,
} from "lucide-react";
import { useRequireAuth } from "@/lib/auth";
import { cn } from "@/lib/cn";
import { Button } from "./ui";

const NAV = [
  { href: "/", label: "Overview", icon: LayoutDashboard },
  { href: "/devices", label: "Devices", icon: Monitor },
  { href: "/jobs", label: "Jobs", icon: ScrollText },
  { href: "/policies", label: "Policies", icon: Shield },
  { href: "/groups", label: "Groups", icon: Users },
  { href: "/alerts", label: "Alerts", icon: Bell },
  { href: "/reports", label: "Reports", icon: FileText },
  { href: "/software", label: "Inventory", icon: Box },
  { href: "/settings", label: "Settings", icon: Settings },
];

export function Chrome({ children }: { children: React.ReactNode }) {
  const { user, ready, logout, isAdmin } = useRequireAuth();
  const pathname = usePathname();

  if (!ready || !user) {
    return (
      <div className="flex min-h-screen items-center justify-center text-sm text-mute">
        Loading…
      </div>
    );
  }

  return (
    <div className="flex min-h-screen">
      <aside className="sticky top-0 flex h-screen w-56 shrink-0 flex-col border-r border-line bg-panel">
        <div className="flex items-center gap-2 px-4 py-4">
          <Activity className="h-5 w-5 text-accent" />
          <div>
            <div className="text-sm font-semibold tracking-wide">OSFM-EDM</div>
            <div className="text-[10px] uppercase tracking-widest text-mute">Console</div>
          </div>
        </div>
        <nav className="flex-1 space-y-0.5 px-2">
          {NAV.map((item) => {
            const active =
              item.href === "/"
                ? pathname === "/"
                : pathname === item.href || pathname.startsWith(`${item.href}/`);
            const Icon = item.icon;
            return (
              <Link
                key={item.href}
                href={item.href}
                className={cn(
                  "flex items-center gap-2 rounded-md px-3 py-2 text-sm transition",
                  active
                    ? "bg-raised text-accent"
                    : "text-mute hover:bg-raised hover:text-white",
                )}
              >
                <Icon className="h-4 w-4" />
                {item.label}
              </Link>
            );
          })}
        </nav>
        <div className="border-t border-line p-3">
          <div className="truncate text-sm">{user.username}</div>
          <div className="mb-2 text-[11px] uppercase tracking-wide text-mute">{user.role}</div>
          <Button variant="ghost" className="w-full justify-start px-0" onClick={() => logout()}>
            Sign out
          </Button>
          {!isAdmin && (
            <p className="mt-2 text-[11px] text-mute">Viewer — read only</p>
          )}
        </div>
      </aside>
      <main className="min-w-0 flex-1 px-6 py-6">{children}</main>
    </div>
  );
}

export function PageHeader({
  title,
  subtitle,
  actions,
}: {
  title: string;
  subtitle?: string;
  actions?: React.ReactNode;
}) {
  return (
    <div className="mb-6 flex flex-wrap items-end justify-between gap-3">
      <div>
        <h1 className="text-xl font-semibold tracking-tight">{title}</h1>
        {subtitle && <p className="mt-1 text-sm text-mute">{subtitle}</p>}
      </div>
      {actions}
    </div>
  );
}
