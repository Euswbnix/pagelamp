import { type ReactNode, useId } from "react";
import { useLampBand } from "@/components/layout/lamp";
import { cn } from "@/lib/utils";

interface SectionProps {
  title: ReactNode;
  description?: ReactNode;
  /** Controls shown on the right of the heading. */
  actions?: ReactNode;
  /**
   * The section shows "now" and has loaded: light the lamp band (§6.1), with its word — this
   * section's title — beside the light (and beside the lamp rule when it can't glow).
   */
  lit?: boolean;
  children: ReactNode;
}

/**
 * A titled part of the page: the title, then a hairline, then the content (no card: the page is
 * flat paper, docs/design/macos-shell.md §4.4). Labelled by its h2, so screen readers list it as a
 * region.
 */
export function Section({ title, description, actions, lit = false, children }: SectionProps) {
  const id = useId();
  useLampBand(lit);
  return (
    <section aria-labelledby={id}>
      <div className="flex flex-wrap items-end justify-between gap-x-4 gap-y-2 border-b border-rule pb-3">
        <div className={cn("min-w-0 space-y-1", lit && "pl-lamp-text")}>
          <h2 id={id} className="font-heading text-lg font-semibold tracking-tight">
            {title}
          </h2>
          {description ? <div className="text-sm text-muted-foreground">{description}</div> : null}
        </div>
        {actions ? <div className="flex items-center gap-2">{actions}</div> : null}
      </div>
      <div className="pt-3">{children}</div>
    </section>
  );
}
