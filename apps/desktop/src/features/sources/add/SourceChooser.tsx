import { BadgeCheck, FolderOpen, School, UserRound } from "lucide-react";
import { useId } from "react";
import { useTranslation } from "react-i18next";
import { Badge } from "@/components/ui/badge";
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldLabel,
  FieldTitle,
} from "@/components/ui/field";
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group";

export type SourceChoice = "folderFeed" | "canvas";

const OPTIONS = [
  { value: "folderFeed", icon: FolderOpen, badgeIcon: BadgeCheck },
  { value: "canvas", icon: School, badgeIcon: UserRound },
] as const;

/** The two ways to add course data, as selectable cards with radio semantics. */
export function SourceChooser({
  value,
  onChange,
}: {
  value: SourceChoice;
  onChange: (choice: SourceChoice) => void;
}) {
  const { t } = useTranslation("sources");
  const baseId = useId();
  return (
    <RadioGroup
      value={value}
      onValueChange={(next) => onChange(next as SourceChoice)}
      aria-label={t("chooser.label")}
      className="grid gap-3 sm:grid-cols-2"
    >
      {OPTIONS.map(({ value: option, icon: Icon, badgeIcon: BadgeIcon }) => {
        const id = `${baseId}-${option}`;
        return (
          <FieldLabel key={option} htmlFor={id}>
            <Field orientation="horizontal">
              <FieldContent className="gap-2">
                <FieldTitle id={`${id}-title`}>
                  <Icon className="size-4 text-muted-foreground" aria-hidden />
                  {t(`chooser.${option}.title`)}
                </FieldTitle>
                <Badge
                  id={`${id}-badge`}
                  variant={option === "folderFeed" ? "secondary" : "outline"}
                >
                  <BadgeIcon aria-hidden />
                  {t(`chooser.${option}.badge`)}
                </Badge>
                <FieldDescription id={`${id}-description`}>
                  {t(`chooser.${option}.description`)}
                </FieldDescription>
              </FieldContent>
              <RadioGroupItem
                value={option}
                id={id}
                aria-labelledby={`${id}-title`}
                aria-describedby={`${id}-badge ${id}-description`}
              />
            </Field>
          </FieldLabel>
        );
      })}
    </RadioGroup>
  );
}
