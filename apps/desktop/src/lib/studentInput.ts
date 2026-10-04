/**
 * What counts as the student doing something in the window: a press (mouse, pen or touch), a
 * click (assistive technology activates a control with a click and no press before it), a key
 * going down, the wheel.
 *
 * Not a pointer move: the engines make up moves of their own (to refresh what is under the
 * pointer after a layout, or when the window is uncovered), and a mouse can drift with nobody
 * there. Not a key that repeats: something resting on the keyboard repeats for hours. And not a
 * key nobody presses to use the app: the keys that tools press to keep a computer awake, a
 * headset's buttons, a modifier alone.
 */
const INPUT_EVENTS = ["pointerdown", "click", "keydown", "wheel"] as const;

/** `KeyboardEvent.key` values that say nothing about a student being at the app. */
const OTHER_KEYS =
  /^(Shift|Control|Alt|AltGraph|Meta|OS|Fn|FnLock|Hyper|Super|Symbol|SymbolLock|CapsLock|NumLock|ScrollLock|Unidentified|F1[3-9]|F2[0-4]|Power|PowerOff|Sleep|Standby|WakeUp|Eject|(Media|Audio|Launch|Brightness|Microphone|Speech).*)$/;

export function isStudentInput(
  event: { type: string; isTrusted: boolean; repeat?: boolean; key?: string },
  trustedOnly: boolean,
): boolean {
  if (trustedOnly && !event.isTrusted) return false;
  if (event.type !== "keydown") return true;
  return event.repeat !== true && !OTHER_KEYS.test(event.key ?? "");
}

/**
 * Calls `onInput` at each such input anywhere in the window; returns a function that stops
 * listening. With `trustedOnly`, an event a script made doesn't count: only the browser's own.
 */
export function listenForStudentInput(onInput: () => void, trustedOnly: boolean): () => void {
  const listener = (event: Event) => {
    if (isStudentInput(event as KeyboardEvent, trustedOnly)) onInput();
  };
  // Capture: seen before anything on the page can stop it. Passive: scrolling never waits.
  const options = { capture: true, passive: true } as const;
  for (const type of INPUT_EVENTS) window.addEventListener(type, listener, options);
  return () => {
    for (const type of INPUT_EVENTS) window.removeEventListener(type, listener, options);
  };
}
