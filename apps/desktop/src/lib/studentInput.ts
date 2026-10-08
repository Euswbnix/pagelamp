/**
 * What counts as the student doing something in the window: a press (mouse, pen or touch), a
 * click (assistive technology activates a control with a click and no press before it), a key
 * going down.
 *
 * Not the wheel: the system sends scrolling to the window under the pointer, in front or not,
 * and what a trackpad was given a moment ago goes on arriving by itself, in whichever window
 * is under the pointer by then.
 * Not a pointer move: the engines make up moves of their own (to refresh what is under the
 * pointer after a layout, or when the window is uncovered), and a mouse can drift with nobody
 * there. Not a key that repeats, nor the clicks it makes on the control it rests on: something
 * resting on the keyboard repeats for hours. Not a key nobody presses to use the app: the keys
 * that tools press to keep a computer awake, a headset's buttons, a modifier alone. And not the
 * key that switches apps (Tab with Command, Alt or the Windows key held): it is pressed to come
 * to this window or to leave it, not in it.
 */
export type StudentInput = "press" | "click" | "key";

/** What a press or a key going down ends with. */
export type StudentRelease = "pointer" | "key";

// A menu of the system's and a drag take the pointer's release for themselves: what opens
// the one and ends the other stands in for it.
const RELEASES: Record<string, StudentRelease> = {
  pointerup: "pointer",
  pointercancel: "pointer",
  contextmenu: "pointer",
  dragend: "pointer",
  keyup: "key",
};
const HEARD = ["pointerdown", "click", "keydown", ...Object.keys(RELEASES)] as const;

/** `KeyboardEvent.key` values that say nothing about a student being at the app. */
const OTHER_KEYS =
  /^(Shift|Control|Alt|AltGraph|Meta|OS|Fn|FnLock|Hyper|Super|Symbol|SymbolLock|CapsLock|NumLock|ScrollLock|Unidentified|F1[3-9]|F2[0-4]|Power|PowerOff|Sleep|Standby|WakeUp|Eject|(Media|Audio|Launch|Brightness|Microphone|Speech).*)$/;

/** Command, or the Windows key. */
const COMMAND = /^(Meta|OS)$/;

interface HeardEvent {
  type: string;
  isTrusted: boolean;
  repeat?: boolean;
  key?: string;
  metaKey?: boolean;
  altKey?: boolean;
}

/**
 * The kind of input `event` is, or null when it isn't the student doing something. With
 * `trustedOnly`, only what the browser itself dispatched for a device counts, never an event a
 * script made.
 */
export function studentInput(event: HeardEvent, trustedOnly: boolean): StudentInput | null {
  if (trustedOnly && !event.isTrusted) return null;
  if (event.type === "pointerdown") return "press";
  if (event.type === "click") return "click";
  if (event.type !== "keydown") return null;
  if (event.repeat === true || OTHER_KEYS.test(event.key ?? "")) return null;
  const switchesApps = event.key === "Tab" && (event.metaKey === true || event.altKey === true);
  return switchesApps ? null : "key";
}

/**
 * Calls `onInput` with the kind of each input, wherever in the window it happens, and
 * `onRelease` when a pointer or a key comes up again, or when the page will not hear that it
 * did. A modifier coming up releases nothing (the key it was held with may still be down),
 * except Command: while it is held, macOS never reports the other key's release. Returns a
 * function that stops listening.
 */
export function listenForStudentInput(
  onInput: (kind: StudentInput) => void,
  trustedOnly: boolean,
  onRelease?: (what: StudentRelease) => void,
): () => void {
  // A key is repeating: the clicks that come without a pointer (detail 0) are its own.
  let repeating = false;
  const listener = (event: Event) => {
    if (trustedOnly && !event.isTrusted) return;
    const { type, key = "", repeat } = event as KeyboardEvent;
    if (type === "keydown") repeating = repeat === true;
    if (type === "keyup") repeating = false;
    const released = RELEASES[type];
    if (released) {
      const modifier = type === "keyup" && OTHER_KEYS.test(key) && !COMMAND.test(key);
      if (!modifier) onRelease?.(released);
      return;
    }
    const kind = studentInput(event as KeyboardEvent, false);
    if (kind === "click" && repeating && (event as MouseEvent).detail === 0) return;
    if (kind) onInput(kind);
  };
  // Capture: seen before anything on the page can stop it. Passive: scrolling never waits.
  const options = { capture: true, passive: true } as const;
  for (const type of HEARD) window.addEventListener(type, listener, options);
  return () => {
    for (const type of HEARD) window.removeEventListener(type, listener, options);
  };
}
