import { Check, Copy } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";

interface CopyButtonProps {
  text: string;
  /** Accessible name, e.g. "Copy Claude Desktop config". Defaults to "Copy". */
  label?: string;
  size?: "sm" | "default";
  variant?: "outline" | "ghost" | "secondary";
}

/** Copies `text` to the clipboard and confirms with a check mark for 2 seconds. */
export function CopyButton({ text, label, size = "sm", variant = "outline" }: CopyButtonProps) {
  const { t } = useTranslation();
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    if (!copied) return;
    const timer = setTimeout(() => setCopied(false), 2000);
    return () => clearTimeout(timer);
  }, [copied]);

  async function copy() {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
    } catch {
      toast.error(t("actions.copyFailed"));
    }
  }

  return (
    <Button type="button" size={size} variant={variant} onClick={copy} aria-label={label}>
      {copied ? <Check aria-hidden /> : <Copy aria-hidden />}
      <span aria-live="polite">{copied ? t("actions.copied") : t("actions.copy")}</span>
    </Button>
  );
}
