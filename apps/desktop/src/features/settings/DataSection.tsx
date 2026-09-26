import { ArrowRight } from "lucide-react";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";
import { useApi } from "@/api/context";
import { useStatus } from "@/api/queries";
import type { AppStatus, StoreCounts } from "@/api/types";
import { CopyButton } from "@/components/common/CopyButton";
import { ErrorState } from "@/components/common/ErrorState";
import { buttonVariants } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { paths } from "@/lib/routes";
import { RevealFolderButton } from "./RevealFolderButton";
import { SettingsSection } from "./SettingsSection";

/** Where the data lives on disk and how much of it there is. */
export function DataSection() {
  const { t } = useTranslation("settings");
  const status = useStatus();
  return (
    <SettingsSection title={t("data.title")} description={t("data.description")}>
      {status.isPending ? (
        <DataSkeleton />
      ) : status.isError ? (
        <ErrorState error={status.error} onRetry={() => void status.refetch()} />
      ) : (
        <DataDetails status={status.data} />
      )}
    </SettingsSection>
  );
}

function DataDetails({ status }: { status: AppStatus }) {
  const { t } = useTranslation("settings");
  const api = useApi();
  return (
    <>
      <dl className="space-y-4">
        <PathRow
          label={t("data.folder")}
          path={status.data_dir}
          copyLabel={t("data.copyFolder")}
          action={
            <RevealFolderButton label={t("data.reveal")} reveal={() => api.revealDataDir()} />
          }
        />
        <PathRow
          label={t("data.database")}
          path={status.db_path}
          copyLabel={t("data.copyDatabase")}
        />
      </dl>
      <CountList counts={status.counts} />
      {status.sources.length === 0 ? <NothingSynced /> : null}
    </>
  );
}

interface PathRowProps {
  label: string;
  path: string;
  copyLabel: string;
  action?: ReactNode;
}

function PathRow({ label, path, copyLabel, action }: PathRowProps) {
  return (
    <div className="space-y-1.5">
      <dt className="text-sm font-medium">{label}</dt>
      <dd className="flex flex-wrap items-center gap-2">
        <code className="min-w-0 flex-1 rounded-md border bg-muted/50 px-3 py-1.5 font-mono text-[13px] break-all">
          {path}
        </code>
        <CopyButton text={path} label={copyLabel} />
        {action}
      </dd>
    </div>
  );
}

// Which StoreCounts field each tile shows, and its label key under data.counts.*
// (`chunks` and `modules` are internal details, so they're left out).
const COUNTS = [
  { key: "courses", label: "courses" },
  { key: "hidden_courses", label: "hiddenCourses" },
  { key: "materials", label: "materials" },
  { key: "indexed_materials", label: "readable" },
  { key: "events", label: "events" },
  { key: "study_plans", label: "studyPlans" },
] as const satisfies readonly { key: keyof StoreCounts; label: string }[];

function CountList({ counts }: { counts: StoreCounts }) {
  const { t, i18n } = useTranslation("settings");
  const number = new Intl.NumberFormat(i18n.language);
  return (
    <div className="space-y-2">
      <h3 className="text-sm font-medium">{t("data.countsTitle")}</h3>
      <dl className="grid grid-cols-2 gap-3 sm:grid-cols-3">
        {COUNTS.map(({ key, label }) => (
          <div key={key} className="rounded-lg border px-3 py-2">
            <dt className="text-xs text-muted-foreground">{t(`data.counts.${label}`)}</dt>
            <dd className="text-lg font-semibold tabular-nums">{number.format(counts[key])}</dd>
          </div>
        ))}
      </dl>
    </div>
  );
}

function NothingSynced() {
  const { t } = useTranslation("settings");
  return (
    <p className="flex flex-wrap items-center gap-x-3 gap-y-1 text-sm text-muted-foreground">
      {t("data.nothingYet")}
      <Link to={paths.sources} className={buttonVariants({ variant: "link", size: "sm" })}>
        {t("data.addSource")}
        <ArrowRight aria-hidden />
      </Link>
    </p>
  );
}

function DataSkeleton() {
  const { t } = useTranslation();
  return (
    <div aria-busy="true" className="space-y-4">
      <span className="sr-only" role="status">
        {t("states.loading")}
      </span>
      {["folder", "db"].map((key) => (
        <div key={key} className="space-y-1.5" aria-hidden>
          <Skeleton className="h-4 w-28" />
          <Skeleton className="h-8 w-full" />
        </div>
      ))}
      <div className="grid grid-cols-2 gap-3 sm:grid-cols-3" aria-hidden>
        {["a", "b", "c", "d", "e", "f"].map((key) => (
          <Skeleton key={key} className="h-14" />
        ))}
      </div>
    </div>
  );
}
