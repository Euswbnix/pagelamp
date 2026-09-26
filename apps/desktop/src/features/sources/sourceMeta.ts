// Small helpers about sources shared by the Sources screen, the add forms and onboarding.

import { CalendarDays, FolderOpen, School } from "lucide-react";
import type { SourceKind, SourceRecord } from "@/api/types";

export const SOURCE_ICON: Record<SourceKind, typeof FolderOpen> = {
  canvas: School,
  folder: FolderOpen,
  ical: CalendarDays,
};

/** Reads a string field of a source's non-secret config (untyped JSON from the backend). */
export function configString(source: SourceRecord, key: string): string | null {
  const value = source.config[key];
  return typeof value === "string" && value.trim() !== "" ? value : null;
}

/** Canvas and calendar-feed sources keep a secret (token / feed URL) that can be replaced. */
export function hasSecret(kind: SourceKind): kind is "canvas" | "ical" {
  return kind === "canvas" || kind === "ical";
}
