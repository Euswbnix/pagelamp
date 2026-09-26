import { useTranslation } from "react-i18next";
import { useStatus } from "@/api/queries";
import type { BrandLinks } from "@/brand";
import { brand, localized } from "@/brand";
import { BrandMark } from "@/components/common/BrandMark";
import { ExternalLink } from "@/components/common/ExternalLink";
import { Skeleton } from "@/components/ui/skeleton";
import { isHttpUrl } from "@/lib/url";
import { SettingsSection } from "./SettingsSection";

// SPDX identifier of the project license (root LICENSE, docs/ARCHITECTURE.md §7).
const LICENSE = "Apache-2.0";

const LINK_KEYS: (keyof BrandLinks)[] = ["homepage", "help", "issues"];

/** Product name, tagline, version, license and the brand's links (only those that are set). */
export function AboutSection() {
  const { t, i18n } = useTranslation("settings");
  const links = LINK_KEYS.flatMap((key) => {
    const href = brand.links[key];
    return isHttpUrl(href) ? [{ key, href }] : [];
  });

  return (
    <SettingsSection title={t("about.title")}>
      <div className="space-y-1.5">
        <BrandMark />
        <p className="text-sm text-muted-foreground">{localized(brand.tagline, i18n.language)}</p>
        {brand.distributedBy ? (
          <p className="text-sm text-muted-foreground">
            {localized(brand.distributedBy, i18n.language)}
          </p>
        ) : null}
      </div>
      <dl className="grid grid-cols-[auto_1fr] gap-x-6 gap-y-2 text-sm">
        <dt className="text-muted-foreground">{t("about.version")}</dt>
        <dd>
          <Version />
        </dd>
        <dt className="text-muted-foreground">{t("about.license")}</dt>
        <dd>{LICENSE}</dd>
      </dl>
      {links.length > 0 ? (
        <nav aria-label={t("about.links")}>
          <ul className="flex flex-wrap gap-x-5 gap-y-2 text-sm">
            {links.map(({ key, href }) => (
              <li key={key}>
                <ExternalLink href={href}>{t(`about.${key}`)}</ExternalLink>
              </li>
            ))}
          </ul>
        </nav>
      ) : null}
    </SettingsSection>
  );
}

function Version() {
  const { t } = useTranslation("settings");
  const status = useStatus();
  if (status.isPending) return <Skeleton className="h-4 w-20" />;
  if (status.isError)
    return <span className="text-muted-foreground">{t("about.unavailable")}</span>;
  return <span className="font-mono">{status.data.version}</span>;
}
