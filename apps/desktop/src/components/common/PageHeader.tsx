import { type ReactNode, useRef } from "react";
import { createPortal } from "react-dom";
import { useScrolledUnder, useToolbar } from "@/components/layout/toolbar";

interface PageHeaderProps {
  title: ReactNode;
  description?: ReactNode;
  /** Buttons shown on the right (in the toolbar row, inside the app shell). */
  actions?: ReactNode;
  /** Small line above the title (e.g. breadcrumb or course code). */
  eyebrow?: ReactNode;
}

/**
 * The top of every screen: one h1, an optional description and actions. Inside the app shell
 * the actions sit in the sticky toolbar row (docs/design/macos-shell.md §8), which also shows
 * a small copy of the title once the h1 has scrolled under it.
 */
export function PageHeader({ title, description, actions, eyebrow }: PageHeaderProps) {
  const toolbar = useToolbar();
  const headingRef = useRef<HTMLHeadingElement>(null);
  const titleUnder = useScrolledUnder(headingRef, toolbar?.scroller ?? null);
  return (
    <>
      {toolbar
        ? createPortal(
            <>
              {/* A visual echo of the h1; screen readers have the heading itself. */}
              <span
                aria-hidden
                className="pl-toolbar-title min-w-0 truncate font-semibold"
                data-shown={titleUnder || undefined}
              >
                {title}
              </span>
              {actions ? (
                <div className="ml-auto flex shrink-0 items-center gap-2">{actions}</div>
              ) : null}
            </>,
            toolbar.slot,
          )
        : null}
      <header className="flex flex-wrap items-start justify-between gap-4 pb-6">
        <div className="min-w-0 space-y-1">
          {eyebrow ? <div className="text-sm text-muted-foreground">{eyebrow}</div> : null}
          <h1
            ref={headingRef}
            className="font-heading text-2xl font-semibold tracking-tight text-balance"
          >
            {title}
          </h1>
          {description ? (
            <p className="max-w-prose text-sm text-muted-foreground">{description}</p>
          ) : null}
        </div>
        {actions && !toolbar ? (
          <div className="flex shrink-0 items-center gap-2">{actions}</div>
        ) : null}
      </header>
    </>
  );
}
