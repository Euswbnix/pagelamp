import { type ReactNode, useId } from "react";
import { cn } from "@/lib/utils";

interface SectionProps {
  title: ReactNode;
  description?: ReactNode;
  /** Controls shown on the right of the heading. */
  actions?: ReactNode;
  /** Draw the section as a card (for the prominent blocks at the top). */
  card?: boolean;
  children: ReactNode;
}

/** A titled part of the page. Labelled by its h2, so screen readers list it as a region. */
export function Section({ title, description, actions, card, children }: SectionProps) {
  const id = useId();
  return (
    <section
      aria-labelledby={id}
      className={cn(
        "space-y-4",
        card && "rounded-xl bg-card p-5 text-card-foreground ring-1 ring-foreground/10",
      )}
    >
      <div className="flex flex-wrap items-start justify-between gap-x-4 gap-y-2">
        <div className="min-w-0 space-y-1">
          <h2 id={id} className="font-heading text-lg font-semibold tracking-tight">
            {title}
          </h2>
          {description ? <div className="text-sm text-muted-foreground">{description}</div> : null}
        </div>
        {actions ? <div className="flex items-center gap-2">{actions}</div> : null}
      </div>
      {children}
    </section>
  );
}
