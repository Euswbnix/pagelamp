import { BookOpenText, FolderPlus } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";
import { useStatus } from "@/api/queries";
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
import { SyncNowButton } from "./SyncNowButton";

/** No courses at all. Offers "Add a source", and "Sync now" when sources exist already. */
export function CoursesEmpty() {
  const { t } = useTranslation("courses");
  const status = useStatus();
  const hasSources = (status.data?.sources.length ?? 0) > 0;

  return (
    <Empty className="border p-10">
      <EmptyHeader>
        <EmptyMedia variant="icon">
          <BookOpenText aria-hidden />
        </EmptyMedia>
        <EmptyTitle>{t("empty.title")}</EmptyTitle>
        <EmptyDescription>
          {hasSources ? t("empty.descriptionHasSources") : t("empty.description")}
        </EmptyDescription>
      </EmptyHeader>
      <EmptyContent>
        <div className="flex flex-wrap justify-center gap-2">
          <Button asChild variant={hasSources ? "outline" : "default"}>
            <Link to={paths.welcome}>
              <FolderPlus aria-hidden />
              {t("empty.addSource")}
            </Link>
          </Button>
          <SyncNowButton />
        </div>
      </EmptyContent>
    </Empty>
  );
}
