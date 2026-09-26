import { useMutation } from "@tanstack/react-query";
import { FolderOpen, LoaderCircle } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { toApiError } from "@/api/errors";
import { Button } from "@/components/ui/button";

interface RevealFolderButtonProps {
  label: string;
  /** A desktop helper that opens one fixed folder (data, logs); never takes a path from the UI. */
  reveal: () => Promise<void>;
}

/** Opens one of Weekmark's folders in Finder / Explorer. */
export function RevealFolderButton({ label, reveal }: RevealFolderButtonProps) {
  const { t } = useTranslation("settings");
  const { t: tc } = useTranslation();
  const mutation = useMutation({
    mutationFn: reveal,
    onError: (error) => {
      toast.error(t("data.revealFailed"), {
        description: tc(`errors.${toApiError(error).kind}`),
      });
    },
  });

  return (
    <Button
      type="button"
      variant="outline"
      size="sm"
      onClick={() => mutation.mutate()}
      disabled={mutation.isPending}
    >
      {mutation.isPending ? (
        <LoaderCircle className="animate-spin" aria-hidden />
      ) : (
        <FolderOpen aria-hidden />
      )}
      {label}
    </Button>
  );
}
