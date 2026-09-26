import type { ReactNode } from "react";
import type { SourceRecord } from "@/api/types";

/** Props shared by the add-source forms (used in onboarding and the Add source dialog). */
export interface AddFormProps {
  /** Text of the submit button ("Add and continue", "Add source"). */
  submitLabel: string;
  /** Called once everything the student asked for was added. */
  onAdded: (records: SourceRecord[]) => void;
  /** Shown left of the submit button, e.g. a Back or Cancel button. */
  footerStart?: ReactNode;
}
