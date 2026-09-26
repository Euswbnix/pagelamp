import { CircleAlert } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toApiError } from "@/api/errors";
import { Button } from "@/components/ui/button";
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty";

interface ErrorStateProps {
  error: unknown;
  onRetry?: () => void;
  title?: string;
}

/** Friendly error block for a failed query. Explains by error kind; shows the backend message. */
export function ErrorState({ error, onRetry, title }: ErrorStateProps) {
  const { t } = useTranslation();
  const apiError = toApiError(error);
  return (
    <Empty role="alert" className="border">
      <EmptyHeader>
        <EmptyMedia variant="icon">
          <CircleAlert aria-hidden />
        </EmptyMedia>
        <EmptyTitle>{title ?? t("states.errorTitle")}</EmptyTitle>
        <EmptyDescription>{t(`errors.${apiError.kind}`)}</EmptyDescription>
        {apiError.message ? (
          <EmptyDescription lang="en" className="text-xs">
            {apiError.message}
          </EmptyDescription>
        ) : null}
      </EmptyHeader>
      {onRetry ? (
        <EmptyContent>
          <Button variant="outline" onClick={onRetry}>
            {t("actions.retry")}
          </Button>
        </EmptyContent>
      ) : null}
    </Empty>
  );
}
