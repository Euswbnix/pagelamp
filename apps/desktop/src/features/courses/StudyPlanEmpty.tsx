import { Cable, NotebookPen } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";
import { CopyButton } from "@/components/common/CopyButton";
import { Button } from "@/components/ui/button";
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty";
import { paths } from "@/lib/routes";

/** No plan saved yet: tell the student what to ask their AI app, and let them copy it. */
export function StudyPlanEmpty() {
  const { t } = useTranslation("courses");
  const prompt = t("plan.prompt");
  return (
    <Empty className="border p-6">
      <EmptyHeader>
        <EmptyMedia variant="icon">
          <NotebookPen aria-hidden />
        </EmptyMedia>
        <EmptyTitle>{t("plan.emptyTitle")}</EmptyTitle>
        <EmptyDescription>{t("plan.emptyDescription", { prompt })}</EmptyDescription>
      </EmptyHeader>
      <EmptyContent>
        <div className="flex flex-wrap justify-center gap-2">
          <CopyButton text={prompt} label={t("plan.copyPrompt")} />
          <Button asChild variant="ghost" size="sm">
            <Link to={paths.connect}>
              <Cable aria-hidden />
              {t("plan.connect")}
            </Link>
          </Button>
        </div>
      </EmptyContent>
    </Empty>
  );
}
