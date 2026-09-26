import { type ReactNode, useId } from "react";
import { Card, CardContent, CardDescription, CardHeader } from "@/components/ui/card";

interface SettingsSectionProps {
  title: string;
  description?: ReactNode;
  children: ReactNode;
}

/** One settings card: an h2 heading (also the section's accessible name) and its content. */
export function SettingsSection({ title, description, children }: SettingsSectionProps) {
  const headingId = useId();
  return (
    <section aria-labelledby={headingId}>
      <Card>
        <CardHeader>
          <h2 id={headingId} className="font-heading text-base leading-snug font-medium">
            {title}
          </h2>
          {description ? <CardDescription>{description}</CardDescription> : null}
        </CardHeader>
        <CardContent className="space-y-5">{children}</CardContent>
      </Card>
    </section>
  );
}
