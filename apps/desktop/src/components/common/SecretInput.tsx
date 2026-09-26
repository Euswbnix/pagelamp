import { Eye, EyeOff } from "lucide-react";
import { type ComponentProps, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

type SecretInputProps = Omit<ComponentProps<typeof Input>, "type" | "autoComplete">;

/**
 * Input for a token or private feed URL. The value lives only in the parent form's local
 * state (never Zustand, localStorage or logs); the parent clears it after submitting.
 */
export function SecretInput(props: SecretInputProps) {
  const { t } = useTranslation();
  const [visible, setVisible] = useState(false);
  return (
    <div className="relative">
      <Input
        {...props}
        type={visible ? "text" : "password"}
        autoComplete="off"
        autoCorrect="off"
        autoCapitalize="off"
        spellCheck={false}
        data-1p-ignore
        data-lpignore="true"
        className="pr-20 font-mono"
      />
      <Button
        type="button"
        size="xs"
        variant="ghost"
        className="absolute top-1/2 right-1.5 -translate-y-1/2"
        onClick={() => setVisible((v) => !v)}
        aria-controls={props.id}
      >
        {visible ? <EyeOff aria-hidden /> : <Eye aria-hidden />}
        {visible ? t("secret.hide") : t("secret.show")}
      </Button>
    </div>
  );
}
