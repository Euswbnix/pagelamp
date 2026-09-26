import { CircleCheck, FolderOpen, LockKeyhole } from "lucide-react";
import { type FormEvent, useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { useApi } from "@/api/context";
import { type ApiError, toApiError } from "@/api/errors";
import { useAddFolderSource, useAddIcalSource } from "@/api/queries";
import type { SourceRecord } from "@/api/types";
import { SecretInput } from "@/components/common/SecretInput";
import { Button } from "@/components/ui/button";
import {
  Field,
  FieldDescription,
  FieldError,
  FieldGroup,
  FieldLabel,
  FieldLegend,
  FieldSeparator,
  FieldSet,
} from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { AddErrorMessage, describedBy } from "./AddErrorMessage";
import { FormFooter } from "./FormFooter";
import type { AddFormProps } from "./types";

/**
 * The recommended, shareable path: a local course folder and/or the LMS calendar feed.
 * Each part is added on its own, so if one fails the other stays added and is not retried.
 */
export function AddFolderFeedForm({ submitLabel, onAdded, footerStart }: AddFormProps) {
  const { t } = useTranslation("sources");
  const api = useApi();
  const addFolder = useAddFolderSource();
  const addFeed = useAddIcalSource();
  const id = useId();

  const [path, setPath] = useState("");
  const [termStart, setTermStart] = useState("");
  const [label, setLabel] = useState("");
  // The feed address is a secret: it lives only in this state (gone when the form unmounts)
  // and is cleared as soon as the backend has stored it.
  const [feedUrl, setFeedUrl] = useState("");

  const [folderAdded, setFolderAdded] = useState<SourceRecord | null>(null);
  const [feedAdded, setFeedAdded] = useState<SourceRecord | null>(null);
  const [folderError, setFolderError] = useState<ApiError | null>(null);
  const [feedError, setFeedError] = useState<ApiError | null>(null);
  const [missing, setMissing] = useState(false);
  const [pending, setPending] = useState(false);

  async function chooseFolder() {
    try {
      const chosen = await api.pickFolder();
      if (chosen) {
        setPath(chosen);
        setFolderError(null);
        setMissing(false);
      }
    } catch (error) {
      setFolderError(toApiError(error));
    }
  }

  async function addFolderPart(): Promise<SourceRecord | null> {
    try {
      const record = await addFolder.mutateAsync({
        path: path.trim(),
        termStart: termStart || null,
        label: label.trim() || null,
      });
      setFolderAdded(record);
      setFolderError(null);
      return record;
    } catch (error) {
      setFolderError(toApiError(error));
      return null;
    }
  }

  async function addFeedPart(): Promise<SourceRecord | null> {
    try {
      // The name field belongs to the folder; the feed keeps the backend's default name.
      const record = await addFeed.mutateAsync({ feedUrl: feedUrl.trim(), label: null });
      setFeedUrl("");
      setFeedAdded(record);
      setFeedError(null);
      return record;
    } catch (error) {
      setFeedError(toApiError(error));
      return null;
    } finally {
      // Drop the mutation (and the feed URL in its variables) from the mutation cache now.
      addFeed.reset();
    }
  }

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (pending) return;
    const wantsFolder = !folderAdded && path.trim() !== "";
    const wantsFeed = !feedAdded && feedUrl.trim() !== "";
    if (!wantsFolder && !wantsFeed && !folderAdded && !feedAdded) {
      setMissing(true);
      return;
    }
    setMissing(false);
    setPending(true);
    const folder = wantsFolder ? await addFolderPart() : folderAdded;
    const feed = wantsFeed ? await addFeedPart() : feedAdded;
    setPending(false);
    const failed = (wantsFolder && !folder) || (wantsFeed && !feed);
    if (!failed) onAdded([folder, feed].filter((r): r is SourceRecord => r !== null));
  }

  const ids = {
    folderHint: `${id}-folder-hint`,
    path: `${id}-path`,
    pathError: `${id}-path-error`,
    termStart: `${id}-term-start`,
    termStartHint: `${id}-term-start-hint`,
    name: `${id}-name`,
    nameHint: `${id}-name-hint`,
    feed: `${id}-feed`,
    feedWhere: `${id}-feed-where`,
    feedPrivate: `${id}-feed-private`,
    feedError: `${id}-feed-error`,
  };

  return (
    <form onSubmit={submit} noValidate aria-busy={pending}>
      <FieldGroup>
        <FieldSet>
          <FieldLegend>{t("folderFeed.folderLegend")}</FieldLegend>
          <FieldDescription id={ids.folderHint}>{t("folderFeed.folderHint")}</FieldDescription>
          <Field data-invalid={folderError ? true : undefined}>
            {folderAdded ? (
              <AddedLine text={t("folderFeed.folderAdded")} detail={path} />
            ) : (
              <>
                <FieldLabel htmlFor={ids.path}>{t("folderFeed.pathLabel")}</FieldLabel>
                <div className="flex gap-2">
                  <Input
                    id={ids.path}
                    value={path}
                    onChange={(e) => setPath(e.target.value)}
                    placeholder={t("folderFeed.pathPlaceholder")}
                    className="font-mono"
                    spellCheck={false}
                    aria-invalid={folderError ? true : undefined}
                    aria-describedby={describedBy(ids.folderHint, folderError && ids.pathError)}
                  />
                  <Button type="button" variant="outline" onClick={chooseFolder}>
                    <FolderOpen aria-hidden />
                    {t("folderFeed.choose")}
                  </Button>
                </div>
              </>
            )}
            <AddErrorMessage id={ids.pathError} target="folder" error={folderError} />
          </Field>
          {folderAdded ? null : (
            <div className="grid gap-4 sm:grid-cols-2">
              <Field>
                <FieldLabel htmlFor={ids.termStart}>{t("folderFeed.termStartLabel")}</FieldLabel>
                <Input
                  id={ids.termStart}
                  type="date"
                  value={termStart}
                  onChange={(e) => setTermStart(e.target.value)}
                  aria-describedby={ids.termStartHint}
                />
                <FieldDescription id={ids.termStartHint}>
                  {t("folderFeed.termStartHint")}
                </FieldDescription>
              </Field>
              <Field>
                <FieldLabel htmlFor={ids.name}>{t("folderFeed.nameLabel")}</FieldLabel>
                <Input
                  id={ids.name}
                  value={label}
                  onChange={(e) => setLabel(e.target.value)}
                  placeholder={t("folderFeed.namePlaceholder")}
                  aria-describedby={ids.nameHint}
                />
                <FieldDescription id={ids.nameHint}>{t("folderFeed.nameHint")}</FieldDescription>
              </Field>
            </div>
          )}
        </FieldSet>

        <FieldSeparator />

        <FieldSet>
          <FieldLegend>{t("folderFeed.feedLegend")}</FieldLegend>
          <FieldDescription>{t("folderFeed.feedHint")}</FieldDescription>
          <Field data-invalid={feedError ? true : undefined}>
            {feedAdded ? (
              <AddedLine text={t("folderFeed.feedAdded")} />
            ) : (
              <>
                <FieldLabel htmlFor={ids.feed}>{t("folderFeed.feedLabel")}</FieldLabel>
                <SecretInput
                  id={ids.feed}
                  value={feedUrl}
                  onChange={(e) => setFeedUrl(e.target.value)}
                  placeholder={t("folderFeed.feedPlaceholder")}
                  aria-invalid={feedError ? true : undefined}
                  aria-describedby={describedBy(
                    ids.feedWhere,
                    ids.feedPrivate,
                    feedError && ids.feedError,
                  )}
                />
                <FieldDescription id={ids.feedWhere}>{t("folderFeed.feedWhere")}</FieldDescription>
                <FieldDescription id={ids.feedPrivate} className="flex items-start gap-1.5">
                  <LockKeyhole className="mt-0.5 size-3.5 shrink-0" aria-hidden />
                  {t("folderFeed.feedPrivate")}
                </FieldDescription>
              </>
            )}
            <AddErrorMessage id={ids.feedError} target="feed" error={feedError} />
          </Field>
        </FieldSet>

        {missing ? <FieldError>{t("folderFeed.missing")}</FieldError> : null}

        <FormFooter
          start={footerStart}
          pending={pending}
          submitLabel={submitLabel}
          pendingLabel={t("folderFeed.submitting")}
        />
      </FieldGroup>
    </form>
  );
}

/** "Folder added ✓" in place of an input that no longer needs editing. */
function AddedLine({ text, detail }: { text: string; detail?: string }) {
  return (
    <p className="flex min-w-0 items-center gap-2 text-sm">
      <CircleCheck className="size-4 shrink-0 text-success" aria-hidden />
      <span className="font-medium">{text}</span>
      {detail ? <span className="truncate font-mono text-muted-foreground">{detail}</span> : null}
    </p>
  );
}
