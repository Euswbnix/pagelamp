import { ExternalLink as ExternalIcon } from "lucide-react";
import type { ReactNode } from "react";
import { useApi } from "@/api/context";
import { isHttpUrl } from "@/lib/url";
import { cn } from "@/lib/utils";

interface ExternalLinkProps {
  href: string;
  children: ReactNode;
  className?: string;
  showIcon?: boolean;
}

/**
 * A link that opens in the student's browser (via the opener plugin in the desktop app).
 * The webview itself never navigates away from the app. Anything that isn't http(s)
 * (file://, webcal:, …) renders as plain text: it is never opened.
 */
export function ExternalLink({ href, children, className, showIcon = true }: ExternalLinkProps) {
  const api = useApi();
  if (!isHttpUrl(href)) return <span className={className}>{children}</span>;
  return (
    <a
      href={href}
      target="_blank"
      rel="noreferrer noopener"
      className={cn(
        "inline-flex items-center gap-1 underline-offset-4 hover:underline focus-visible:underline",
        className,
      )}
      onClick={(event) => {
        event.preventDefault();
        void api.openExternal(href);
      }}
    >
      {children}
      {showIcon ? <ExternalIcon className="size-3.5 shrink-0" aria-hidden /> : null}
    </a>
  );
}
