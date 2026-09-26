import { useState } from "react";
import { AddCanvasForm } from "./AddCanvasForm";
import { AddFolderFeedForm } from "./AddFolderFeedForm";
import { type SourceChoice, SourceChooser } from "./SourceChooser";
import type { AddFormProps } from "./types";

/**
 * Pick a source type, then fill in its form. Used by onboarding step 2 and the
 * "Add source" dialog on Sources & sync. Switching type unmounts the other form, which also
 * discards anything secret typed into it.
 */
export function AddSource(props: AddFormProps) {
  const [choice, setChoice] = useState<SourceChoice>("folderFeed");
  return (
    <div className="flex flex-col gap-6">
      <SourceChooser value={choice} onChange={setChoice} />
      {choice === "folderFeed" ? <AddFolderFeedForm {...props} /> : <AddCanvasForm {...props} />}
    </div>
  );
}
