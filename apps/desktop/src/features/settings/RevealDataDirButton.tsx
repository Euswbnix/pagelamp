import { useMutation } from "@tanstack/react-query";
import { FolderOpen, LoaderCircle } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { useApi } from "@/api/context";
import { toApiError } from "@/api/errors";
import { Button } from "@/components/ui/button";

/** Opens the data folder in Finder / Explorer (a desktop helper, not a facade call). */
export function RevealDataDirButton() {
  const { t } = useTranslation("settings");
  const { t: tc } = useTranslation();
  const api = useApi();
  const reveal = useMutation({
    mutationFn: () => api.revealDataDir(),
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
      onClick={() => reveal.mutate()}
      disabled={reveal.isPending}
    >
      {reveal.isPending ? (
        <LoaderCircle className="animate-spin" aria-hidden />
      ) : (
        <FolderOpen aria-hidden />
      )}
      {t("data.reveal")}
    </Button>
  );
}
