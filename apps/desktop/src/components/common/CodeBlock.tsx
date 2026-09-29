import { CopyButton } from "./CopyButton";

interface CodeBlockProps {
  code: string;
  /** Accessible name for the copy button. */
  copyLabel?: string;
}

/**
 * Monospace snippet with a copy button. The button sits in its own row, so long one-line
 * commands scroll underneath nothing and stay readable.
 */
export function CodeBlock({ code, copyLabel }: CodeBlockProps) {
  return (
    <div className="pl-concentric overflow-hidden bg-muted [--pl-pad:1rem]">
      <div className="flex justify-end border-b bg-muted/60 px-2 py-1.5">
        <CopyButton text={code} label={copyLabel} variant="ghost" />
      </div>
      <pre className="overflow-x-auto p-4 font-mono text-[13px] leading-relaxed">
        <code>{code}</code>
      </pre>
    </div>
  );
}
