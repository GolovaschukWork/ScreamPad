import type { ReactNode } from "react";
import { AppSidebar } from "./AppSidebar";
import type { PageId } from "./navigation";

/** The window layout: a fixed sidebar and a main area whose content scrolls on its own. */
export function AppShell({
  current,
  onNavigate,
  children,
}: {
  current: PageId;
  onNavigate: (page: PageId) => void;
  children: ReactNode;
}) {
  return (
    <div className="flex h-screen min-h-0 bg-background text-foreground">
      <AppSidebar current={current} onNavigate={onNavigate} />
      <main className="flex min-w-0 flex-1 flex-col">{children}</main>
    </div>
  );
}

/** Title row of a page, with optional secondary text and actions on the right. */
export function PageHeader({
  title,
  subtitle,
  actions,
}: {
  title: string;
  subtitle?: string;
  actions?: ReactNode;
}) {
  return (
    <header className="flex shrink-0 flex-wrap items-center justify-between gap-x-4 gap-y-2 border-b px-6 py-4">
      <div className="min-w-0">
        <h1 className="truncate text-xl font-semibold tracking-tight">{title}</h1>
        {subtitle && <p className="text-sm text-muted-foreground">{subtitle}</p>}
      </div>
      {actions && <div className="flex items-center gap-2">{actions}</div>}
    </header>
  );
}

/** The scrollable region under the header. */
export function PageBody({ children }: { children: ReactNode }) {
  return <div className="min-h-0 flex-1 overflow-y-auto px-6 py-5">{children}</div>;
}
