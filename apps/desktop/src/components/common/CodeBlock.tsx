import { CopyButton } from "./CopyButton";

interface CodeBlockProps {
  code: string;
  /** Accessible name for the copy button. */
  copyLabel?: string;
}

/** Monospace, horizontally scrollable snippet with a copy button. */
export function CodeBlock({ code, copyLabel }: CodeBlockProps) {
  return (
    <div className="relative rounded-lg border bg-muted/50">
      <div className="absolute top-2 right-2">
        <CopyButton text={code} label={copyLabel} variant="secondary" />
      </div>
      <pre className="overflow-x-auto p-4 pr-24 font-mono text-[13px] leading-relaxed">
        <code>{code}</code>
      </pre>
    </div>
  );
}
