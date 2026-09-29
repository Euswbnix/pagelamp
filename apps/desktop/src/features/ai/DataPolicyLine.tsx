import type { TFunction } from "i18next";
import { useTranslation } from "react-i18next";
import type { DisclosureFacts } from "@/api/provisional/ai";
import { cn } from "@/lib/utils";

/**
 * The one-line data policy of a backend (design §2.5, Canvas §2E): where it goes, training,
 * retention, age and cost. Built only from the facade's facts.
 */
export function dataPolicyParts(
  facts: DisclosureFacts,
  name: string,
  t: TFunction<"ai">,
): string[] {
  if (facts.on_device) return [t("policy.staysHere"), t("policy.cost.free_on_device")];
  const parts = [t("policy.sentTo", { name }), t(`policy.training.${facts.training.kind}`)];
  const retention = facts.retention;
  parts.push(
    retention.kind === "stored_days"
      ? t("policy.retention.stored_days", { count: retention.days })
      : t(`policy.retention.${retention.kind}`),
  );
  if (facts.min_age) {
    parts.push(
      facts.guardian_permission
        ? t("policy.ageGuardian", { age: facts.min_age })
        : t("policy.age", { age: facts.min_age }),
    );
  }
  parts.push(t(`policy.cost.${facts.cost}`));
  return parts;
}

export function DataPolicyLine({
  facts,
  name,
  className,
}: {
  facts: DisclosureFacts;
  name: string;
  className?: string;
}) {
  const { t } = useTranslation("ai");
  return (
    <p className={cn("text-sm text-muted-foreground", className)}>
      <span className="sr-only">{t("policy.label")}: </span>
      {dataPolicyParts(facts, name, t).join(" · ")}
    </p>
  );
}
