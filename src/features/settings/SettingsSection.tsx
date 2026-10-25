import { useId, type ReactNode } from "react";

/** A titled group of settings rows. */
export function SettingsSection({ title, children }: { title: string; children: ReactNode }) {
  const headingId = useId();
  return (
    <section aria-labelledby={headingId} className="space-y-2">
      <h2 id={headingId} className="px-1 text-xs font-semibold tracking-wide text-muted-foreground uppercase">
        {title}
      </h2>
      <div className="divide-y divide-border rounded-lg border bg-card text-card-foreground">{children}</div>
    </section>
  );
}

/** One setting: its label and optional description on the left, its control on the right. */
export function SettingsRow({
  label,
  htmlFor,
  description,
  children,
}: {
  label: string;
  /** Id of the control the label belongs to. Omit when the control labels itself. */
  htmlFor?: string;
  description?: string;
  children: ReactNode;
}) {
  return (
    <div className="flex flex-wrap items-center justify-between gap-x-6 gap-y-2 px-4 py-3">
      <div className="min-w-0">
        {htmlFor ? (
          <label htmlFor={htmlFor} className="text-sm font-medium">
            {label}
          </label>
        ) : (
          <p className="text-sm font-medium">{label}</p>
        )}
        {description && <p className="text-sm text-muted-foreground">{description}</p>}
      </div>
      <div className="shrink-0">{children}</div>
    </div>
  );
}
