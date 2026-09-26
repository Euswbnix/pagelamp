import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import type { SourceRecord } from "@/api/types";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { useReturnFocus } from "@/lib/focus";
import { useStartSync, useSyncStore } from "@/stores/sync";
import { AddSource } from "./add/AddSource";

/** "Add source" on Sources & sync: the same forms as onboarding, then a sync of what was added. */
export function AddSourceDialog({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const { t } = useTranslation("sources");
  const { t: tc } = useTranslation();
  const startSync = useStartSync();
  const returnFocus = useReturnFocus();

  function added(records: SourceRecord[]) {
    onOpenChange(false);
    if (useSyncStore.getState().running) {
      // Only one sync at a time; the new source joins the next one.
      toast.success(t("addDialog.doneLater"));
      return;
    }
    toast.success(t("addDialog.done"));
    const only = records.length === 1 ? records[0] : undefined;
    void startSync(only?.id);
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-h-[90dvh] overflow-y-auto sm:max-w-xl" {...returnFocus}>
        <DialogHeader>
          <DialogTitle>{t("addDialog.title")}</DialogTitle>
          <DialogDescription>{t("addDialog.description")}</DialogDescription>
        </DialogHeader>
        {/* Unmounted when closed, so anything typed (including secrets) is discarded. */}
        <AddSource
          submitLabel={t("addDialog.submit")}
          onAdded={added}
          footerStart={
            <DialogClose asChild>
              <Button type="button" variant="outline">
                {tc("actions.cancel")}
              </Button>
            </DialogClose>
          }
        />
      </DialogContent>
    </Dialog>
  );
}
