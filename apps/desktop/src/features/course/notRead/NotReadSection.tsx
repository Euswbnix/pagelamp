import { ChevronRight } from "lucide-react";
import { type ReactNode, useEffect, useId, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useSearchParams } from "react-router";
import type { CourseOverview } from "@/api/types";
import { ExternalLink } from "@/components/common/ExternalLink";
import { SentenceWithTime, WHEN } from "@/components/common/SentenceWithTime";
import { Button } from "@/components/ui/button";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { isHttpUrl } from "@/lib/url";
import {
  hasDetails,
  hasNever,
  type NotReadItem,
  needsAttention,
  notReadModel,
  onlyNever,
} from "./notRead";

/** `?show=not-read` on a course's page opens this section (the sync row links here). */
export const SHOW_NOT_READ = "not-read";

/**
 * What PageLamp didn't read of a Canvas course, and why, from what the last full sync recorded
 * (`CourseOverview.coverage`). It is there only for a course with such a record, and it never
 * says that everything was read: the record lists what the sync noted, not what it missed.
 *
 * Nothing here starts a sync or a download. Titles and addresses are Canvas's own (the
 * instructor's text): plain text, and a link only when the address is a web address.
 */
export function NotReadSection({ overview }: { overview: CourseOverview }) {
  const { t } = useTranslation("course");
  const { t: ts } = useTranslation("sources");
  const headingId = useId();
  const section = useRef<HTMLElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const [params, setParams] = useSearchParams();
  const asked = params.get("show") === SHOW_NOT_READ;
  const coverage = overview.coverage ?? null;
  const model = coverage ? notReadModel(coverage) : null;
  // Open at first when something waits for the student or failed (also when the record that
  // says so arrives later); what the student chose stays.
  const [choice, setChoice] = useState<boolean | null>(null);
  const open = choice ?? (model !== null && needsAttention(model));
  const present = model !== null;
  // Sent here by the sync row: open, bring it into view, and spend the request, so that
  // coming back to this tab later doesn't jump here again.
  useEffect(() => {
    if (!asked || !present) return;
    setChoice(true);
    // After the shell has put the new page at its top.
    const frame = requestAnimationFrame(() => {
      section.current?.scrollIntoView?.({ block: "start" });
      trigger.current?.focus({ preventScroll: true });
      setParams(
        (previous) => {
          const next = new URLSearchParams(previous);
          next.delete("show");
          return next;
        },
        { replace: true },
      );
    });
    return () => cancelAnimationFrame(frame);
  }, [asked, present, setParams]);

  if (!model) return null;
  const details = hasDetails(model);
  const pages =
    model.linkedPages > 0 ? ts("progress.course.linkedPages", { count: model.linkedPages }) : null;
  const files =
    model.linkedFiles > 0 ? ts("progress.course.linkedFiles", { count: model.linkedFiles }) : null;
  const linked =
    pages && files ? ts("progress.course.linkedBoth", { pages, files }) : (pages ?? files);
  // Nothing to say is not "everything was read": then there is no section at all.
  if (!details && !model.lists && !model.home && !linked && model.toDownload === 0) return null;

  const title = t("notRead.title");
  return (
    <Collapsible open={open} onOpenChange={setChoice} asChild>
      <section ref={section} aria-labelledby={headingId} className="space-y-2">
        <h2 id={headingId} className="font-heading text-base font-semibold tracking-tight">
          {details ? (
            <CollapsibleTrigger asChild>
              <Button
                ref={trigger}
                variant="ghost"
                size="sm"
                className="-ml-2 h-auto px-2 py-1 font-heading text-base font-semibold tracking-tight"
              >
                <ChevronRight
                  aria-hidden
                  className="transition-transform motion-reduce:transition-none [[data-state=open]_&]:rotate-90"
                />
                {title}
              </Button>
            </CollapsibleTrigger>
          ) : (
            title
          )}
        </h2>
        <div className="space-y-1 text-sm text-muted-foreground">
          {model.writtenAt ? (
            <p>
              <SentenceWithTime text={t("notRead.asOf", { when: WHEN })} iso={model.writtenAt} />
            </p>
          ) : null}
          {model.home ? <p>{t(`notRead.home.${model.home}`)}</p> : null}
          {model.lists ? <p>{t(`notRead.lists.${model.lists}`)}</p> : null}
          {linked ? (
            <>
              <p>{ts("progress.course.linked", { items: linked })}</p>
              <p>{t("notRead.linkedNote")}</p>
            </>
          ) : null}
          {model.toDownload > 0 ? (
            <>
              <p>{t("notRead.download", { count: model.toDownload })}</p>
              <p>{t("download.calloutHint")}</p>
            </>
          ) : null}
          {onlyNever(model) ? <p>{t("notRead.onlyNever")}</p> : null}
        </div>
        {details ? (
          <CollapsibleContent className="space-y-5 pt-2 text-sm">
            {model.mustView.length > 0 ? (
              <Group title={t("notRead.group.yours")}>
                <p className="text-muted-foreground">{t("notRead.mustView")}</p>
                <Items items={model.mustView} />
              </Group>
            ) : null}
            {model.failed.lists.length > 0 ||
            model.failed.pagesAll ||
            model.failed.items.length > 0 ? (
              <Group title={t("notRead.group.failed")}>
                <ul className="list-disc space-y-0.5 pl-5">
                  {model.failed.lists.map((list) => (
                    <li key={list}>{t(`notRead.failed.${list}`)}</li>
                  ))}
                  {model.failed.pagesAll ? <li>{t("notRead.failed.pagesAll")}</li> : null}
                  <ItemRows items={model.failed.items} />
                </ul>
                <p className="text-muted-foreground">{t("notRead.failedNote")}</p>
              </Group>
            ) : null}
            {model.lockedPages.length > 0 ||
            model.lockedFiles > 0 ||
            model.tooLarge > 0 ||
            model.pastLimit.length > 0 ||
            model.cutShort.length > 0 ? (
              <Group title={t("notRead.group.cant")}>
                {model.lockedPages.length > 0 ? (
                  <>
                    <p className="text-muted-foreground">{t("notRead.lockedPages")}</p>
                    <Items items={model.lockedPages} />
                  </>
                ) : null}
                {model.lockedFiles > 0 ? (
                  <p>{t("notRead.lockedFiles", { count: model.lockedFiles })}</p>
                ) : null}
                {model.tooLarge > 0 ? (
                  <p>{t("notRead.tooLarge", { count: model.tooLarge })}</p>
                ) : null}
                {model.pastLimit.length > 0 ? (
                  <>
                    <p className="text-muted-foreground">{t("notRead.pastLimit")}</p>
                    <Items items={model.pastLimit} />
                  </>
                ) : null}
                {model.cutShort.length > 0 ? (
                  <>
                    <p className="text-muted-foreground">{t("notRead.cutShort")}</p>
                    <Items items={model.cutShort} />
                  </>
                ) : null}
              </Group>
            ) : null}
            {model.gone.length > 0 ? (
              <Group title={t("notRead.group.gone")}>
                {/* Their addresses are gone with them: titles only. */}
                <Items items={model.gone} linked={false} />
                <p className="text-muted-foreground">{t("notRead.goneNote")}</p>
              </Group>
            ) : null}
            {hasNever(model) ? (
              <Group title={t("notRead.group.never")}>
                {model.never.assignments ? <p>{t("notRead.never.assignments")}</p> : null}
                {model.never.parts ? <p>{t("notRead.never.parts")}</p> : null}
                {model.never.menu ? <p>{t("notRead.never.menu")}</p> : null}
                {model.never.tools.length > 0 ? (
                  <>
                    <p>{t("notRead.never.tools")}</p>
                    <Items items={model.never.tools} />
                  </>
                ) : null}
                {model.never.otherCourses.length > 0 ? (
                  <>
                    <p>{t("notRead.never.otherCourse")}</p>
                    <Items items={model.never.otherCourses} />
                  </>
                ) : null}
              </Group>
            ) : null}
            {model.other.length > 0 ? (
              <Group title={t("notRead.other")}>
                <Items items={model.other} />
              </Group>
            ) : null}
            {model.more > 0 ? (
              <p className="text-muted-foreground">{t("notRead.more", { count: model.more })}</p>
            ) : null}
          </CollapsibleContent>
        ) : null}
      </section>
    </Collapsible>
  );
}

function Group({ title, children }: { title: string; children: ReactNode }) {
  const id = useId();
  return (
    <section aria-labelledby={id} className="space-y-1.5">
      <h3 id={id} className="text-sm font-medium">
        {title}
      </h3>
      {children}
    </section>
  );
}

function Items({ items, linked = true }: { items: NotReadItem[]; linked?: boolean }) {
  return (
    <ul className="list-disc space-y-0.5 pl-5">
      <ItemRows items={items} linked={linked} />
    </ul>
  );
}

function ItemRows({ items, linked = true }: { items: NotReadItem[]; linked?: boolean }) {
  const { t } = useTranslation("course");
  return (
    <>
      {items.map((item, index) => {
        // Canvas's own title, else the address, else a name for the kind of thing.
        const label = item.home
          ? t("notRead.homeTitle")
          : (item.title ?? item.url ?? t(`notRead.untitled.${item.kind}`));
        return (
          // biome-ignore lint/suspicious/noArrayIndexKey: the entries come in the facade's order and never reorder.
          <li key={index} className="[overflow-wrap:anywhere]">
            {linked && item.url && isHttpUrl(item.url) ? (
              <ExternalLink href={item.url}>{label}</ExternalLink>
            ) : (
              label
            )}
          </li>
        );
      })}
    </>
  );
}
