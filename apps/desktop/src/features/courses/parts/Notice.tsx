import type { ReactNode } from "react";

interface NoticeProps {
  /** A lucide icon element (aria-hidden); the title always carries the meaning too. */
  icon: ReactNode;
  title: ReactNode;
  children?: ReactNode;
  action?: ReactNode;
}

/**
 * A calm one-line status box (syncing, token expired, …). It has no role of its own: the
 * caller decides whether it sits inside a live region.
 */
export function Notice({ icon, title, children, action }: NoticeProps) {
  return (
    <div className="flex flex-wrap items-start gap-x-3 gap-y-2 rounded-lg border bg-card p-3 text-sm">
      <span className="mt-0.5 shrink-0">{icon}</span>
      <div className="min-w-0 flex-1 space-y-0.5">
        <p className="font-medium">{title}</p>
        {children ? <div className="text-muted-foreground">{children}</div> : null}
      </div>
      {action ? <div className="shrink-0">{action}</div> : null}
    </div>
  );
}
