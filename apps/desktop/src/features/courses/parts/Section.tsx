import { type ReactNode, useId } from "react";

interface SectionProps {
  title: ReactNode;
  description?: ReactNode;
  /** Controls shown on the right of the heading. */
  actions?: ReactNode;
  children: ReactNode;
}

/**
 * A titled part of the page: the title, then a hairline, then the content (no card: the page is
 * flat paper, docs/design/macos-shell.md §4.4). Labelled by its h2, so screen readers list it as a
 * region.
 */
export function Section({ title, description, actions, children }: SectionProps) {
  const id = useId();
  return (
    <section aria-labelledby={id}>
      <div className="flex flex-wrap items-end justify-between gap-x-4 gap-y-2 border-b border-rule pb-3">
        <div className="min-w-0 space-y-1">
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
