import { MessageSquareText } from "lucide-react";
import { useId } from "react";
import { useTranslation } from "react-i18next";
import { Card, CardContent, CardHeader } from "@/components/ui/card";

/** Example questions to ask once the AI app is connected. */
export function TryItCard() {
  const { t } = useTranslation("connect");
  const headingId = useId();
  return (
    <section aria-labelledby={headingId}>
      <Card className="bg-muted/40">
        <CardHeader>
          <h2
            id={headingId}
            className="flex items-center gap-2 font-heading text-base leading-snug font-medium"
          >
            <MessageSquareText className="size-4" aria-hidden />
            {t("tryIt.title")}
          </h2>
        </CardHeader>
        <CardContent className="space-y-3">
          <Prompt label={t("tryIt.thenAsk")} text={t("tryIt.promptWeek")} />
          <Prompt label={t("tryIt.or")} text={t("tryIt.promptPlan")} />
        </CardContent>
      </Card>
    </section>
  );
}

function Prompt({ label, text }: { label: string; text: string }) {
  return (
    <div className="space-y-1">
      <p className="text-muted-foreground">{label}</p>
      <blockquote className="pl-concentric bg-muted px-3 py-2 font-medium [--pl-pad:1rem]">
        {text}
      </blockquote>
    </div>
  );
}
