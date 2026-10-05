/**
 * Which of its lists a Canvas course doesn't show (the string key under `common.canvasLists`).
 * PageLamp never asks for such a list: it finds pages and files through modules and links only.
 */
export function listsNotShown(pages: boolean, files: boolean): "both" | "pages" | "files" | null {
  if (pages && files) return "both";
  if (pages) return "pages";
  return files ? "files" : null;
}
