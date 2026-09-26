import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import type { SourceRecord } from "@/api/types";
import { AiDisclosureAcknowledgement } from "@/components/common/AiDisclosureAcknowledgement";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { useReturnFocus } from "@/lib/focus";
import { useStartSync, useSyncStore } from "@/stores/sync";
import { useUiStore } from "@/stores/ui";
import { AddSource } from "./add/AddSource";

/**
 * "Add source" on Sources & sync: the same forms as onboarding, then a sync of what was added.
 * A student who skipped onboarding hasn't seen the AI disclosure yet, so it comes first here.
 */
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
  const acknowledged = useUiStore((s) => s.aiDisclosureAcknowledgedAt !== null);
  // Decided when the dialog opens, so the disclosure stays visible after ticking the box.
  const [needsAcknowledgement, setNeedsAcknowledgement] = useState(!acknowledged);
  const [wasOpen, setWasOpen] = useState(open);
  if (open !== wasOpen) {
    setWasOpen(open);
    if (open) setNeedsAcknowledgement(!acknowledged);
  }

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
        {needsAcknowledgement ? <AiDisclosureAcknowledgement /> : null}
        {/* Unmounted when closed, so anything typed (including secrets) is discarded. */}
        {acknowledged ? (
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
        ) : (
          <DialogFooter>
            <DialogClose asChild>
              <Button type="button" variant="outline">
                {tc("actions.cancel")}
              </Button>
            </DialogClose>
          </DialogFooter>
        )}
      </DialogContent>
    </Dialog>
  );
}
