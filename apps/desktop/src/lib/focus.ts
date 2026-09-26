// Keyboard focus helpers shared by screens with steps and dialogs.

import { useRef } from "react";

/**
 * Moves focus to the screen's h1 so keyboard and screen-reader users continue from the top.
 * Every screen has exactly one h1. The heading is focusable only from script (tabIndex -1)
 * and isn't a control, so it gets no focus ring.
 */
export function focusPageHeading() {
  const heading = document.querySelector<HTMLElement>("h1");
  if (!heading) return;
  heading.tabIndex = -1;
  heading.classList.add("outline-none");
  heading.focus();
}

/**
 * Props for a Radix `DialogContent` / `AlertDialogContent` that put focus back where it was
 * when the dialog opened. Radix only returns focus to its own `<DialogTrigger>`, so a dialog
 * opened from state would otherwise leave focus on <body>. When that element is gone (its
 * card was removed, or its error callout disappeared), focus goes to the page heading.
 */
export function useReturnFocus() {
  const opener = useRef<HTMLElement | null>(null);
  return {
    onOpenAutoFocus: () => {
      const active = document.activeElement;
      opener.current = active instanceof HTMLElement ? active : null;
    },
    onCloseAutoFocus: (event: Event) => {
      event.preventDefault();
      if (opener.current?.isConnected) opener.current.focus();
      else focusPageHeading();
      opener.current = null;
    },
  };
}
