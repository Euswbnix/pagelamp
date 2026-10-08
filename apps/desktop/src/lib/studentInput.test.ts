import { afterEach, expect, it, vi } from "vitest";
import { listenForStudentInput, studentInput } from "./studentInput";

let stop: (() => void) | null = null;
afterEach(() => {
  stop?.();
  stop = null;
  document.body.replaceChildren();
});

it("calls back for a press, a click and a key going down, wherever they happen, with the kind", () => {
  const onInput = vi.fn();
  stop = listenForStudentInput(onInput, false);
  const button = document.body.appendChild(document.createElement("button"));
  // Something on the page that keeps the event to itself doesn't hide it.
  button.addEventListener("pointerdown", (event) => event.stopPropagation());

  button.dispatchEvent(new Event("pointerdown", { bubbles: true }));
  expect(onInput.mock.calls).toEqual([["press"]]);
  button.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  expect(onInput.mock.calls).toEqual([["press"], ["key"]]);
  // A control activated without a press before it (assistive technology).
  button.dispatchEvent(new MouseEvent("click", { bubbles: true }));
  expect(onInput.mock.calls).toEqual([["press"], ["key"], ["click"]]);
});

it("doesn't call back for the wheel: scrolling takes nobody at this window", () => {
  const onInput = vi.fn();
  const add = vi.spyOn(window, "addEventListener");
  try {
    stop = listenForStudentInput(onInput, false);
    expect(add.mock.calls.map(([type]) => type)).not.toContain("wheel");
    expect(add.mock.calls.map(([type]) => type)).not.toContain("scroll");
  } finally {
    add.mockRestore();
  }
  window.dispatchEvent(new WheelEvent("wheel", { deltaY: 40 }));
  window.dispatchEvent(new WheelEvent("wheel", { deltaY: 1, deltaMode: 0 }));
  document.dispatchEvent(new Event("scroll"));
  expect(onInput).not.toHaveBeenCalled();
  expect(studentInput({ type: "wheel", isTrusted: true }, true)).toBeNull();
});

it("doesn't call back for the key that switches apps", () => {
  const onInput = vi.fn();
  stop = listenForStudentInput(onInput, false);
  for (const held of [{ metaKey: true }, { altKey: true }, { altKey: true, shiftKey: true }]) {
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", ...held }));
  }
  expect(onInput).not.toHaveBeenCalled();
  // Tab by itself, or with Shift or Control, moves about in the app.
  for (const held of [{}, { shiftKey: true }, { ctrlKey: true }]) {
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", ...held }));
  }
  expect(onInput.mock.calls).toEqual([["key"], ["key"], ["key"]]);
  // Another key with Command or Alt held is the student's shortcut in the app.
  expect(studentInput({ type: "keydown", isTrusted: true, key: "f", metaKey: true }, true)).toBe(
    "key",
  );
});

it("says when a pointer or a key comes up again", () => {
  const onInput = vi.fn();
  const onRelease = vi.fn();
  stop = listenForStudentInput(onInput, false, onRelease);
  window.dispatchEvent(new Event("pointerup"));
  window.dispatchEvent(new Event("pointercancel"));
  window.dispatchEvent(new KeyboardEvent("keyup", { key: "j" }));
  // Any key (but see the modifiers below).
  window.dispatchEvent(new KeyboardEvent("keyup", { key: "Meta" }));
  expect(onRelease.mock.calls).toEqual([["pointer"], ["pointer"], ["key"], ["key"]]);
  expect(onInput).not.toHaveBeenCalled();
});

it("takes no modifier coming up for a release, except Command", () => {
  const onRelease = vi.fn();
  stop = listenForStudentInput(vi.fn(), false, onRelease);
  // Shift let go after Shift+Tab, while Space is already down on the button.
  for (const key of ["Shift", "Control", "Alt", "AltGraph", "CapsLock"]) {
    window.dispatchEvent(new KeyboardEvent("keyup", { key }));
  }
  expect(onRelease).not.toHaveBeenCalled();
  // With Command held, the other key's release may never be reported (as browsers on macOS
  // are known to do): Command's own stands for it.
  window.dispatchEvent(new KeyboardEvent("keyup", { key: "Meta" }));
  window.dispatchEvent(new KeyboardEvent("keyup", { key: "OS" }));
  expect(onRelease.mock.calls).toEqual([["key"], ["key"]]);
});

it("doesn't call back for the clicks a repeating key makes", () => {
  const onInput = vi.fn();
  stop = listenForStudentInput(onInput, false);
  const button = document.body.appendChild(document.createElement("button"));
  // Something rests on Enter while a button has the focus: each repeat presses the button.
  button.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", repeat: true, bubbles: true }));
  button.dispatchEvent(new MouseEvent("click", { bubbles: true, detail: 0 }));
  button.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", repeat: true, bubbles: true }));
  button.dispatchEvent(new MouseEvent("click", { bubbles: true, detail: 0 }));
  expect(onInput).not.toHaveBeenCalled();
  // A click of the mouse meanwhile is somebody's.
  button.dispatchEvent(new MouseEvent("click", { bubbles: true, detail: 1 }));
  expect(onInput.mock.calls).toEqual([["click"]]);
  // The key comes up: a click with no pointer is assistive technology's again.
  button.dispatchEvent(new KeyboardEvent("keyup", { key: "Enter", bubbles: true }));
  button.dispatchEvent(new MouseEvent("click", { bubbles: true, detail: 0 }));
  expect(onInput.mock.calls).toEqual([["click"], ["click"]]);
  // And Enter pressed once is a key and its click.
  button.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  button.dispatchEvent(new MouseEvent("click", { bubbles: true, detail: 0 }));
  expect(onInput.mock.calls).toEqual([["click"], ["click"], ["key"], ["click"]]);
});

it("lets go of a repeating key when the window loses focus", () => {
  const onInput = vi.fn();
  let lost: () => void = () => {};
  const stopped = vi.fn();
  stop = listenForStudentInput(onInput, false, undefined, (onBlur) => {
    lost = onBlur;
    return stopped;
  });
  const button = document.body.appendChild(document.createElement("button"));
  // A key repeats as the window goes; its release is never heard here.
  button.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", repeat: true }));
  lost();
  // Back in the window, a click with no press before it (assistive technology) is heard.
  button.dispatchEvent(new MouseEvent("click", { bubbles: true, detail: 0 }));
  expect(onInput.mock.calls).toEqual([["click"]]);

  expect(stopped).not.toHaveBeenCalled();
  stop();
  stop = null;
  expect(stopped).toHaveBeenCalledTimes(1);
});

it("takes a context menu and the end of a drag for the pointer's release", () => {
  // A menu of the system's may keep the release of the press that opened it, and a drag may
  // end without one.
  const onRelease = vi.fn();
  stop = listenForStudentInput(vi.fn(), false, onRelease);
  window.dispatchEvent(new MouseEvent("contextmenu"));
  expect(onRelease.mock.calls).toEqual([["pointer"]]);
  window.dispatchEvent(new Event("dragend"));
  expect(onRelease.mock.calls).toEqual([["pointer"], ["pointer"]]);
  // Where a drag starts or passes says nothing about the pointer.
  window.dispatchEvent(new Event("dragstart"));
  window.dispatchEvent(new Event("dragover"));
  expect(onRelease).toHaveBeenCalledTimes(2);
});

it("doesn't call back for a pointer that moves, a key that repeats, or anything else", () => {
  const onInput = vi.fn();
  stop = listenForStudentInput(onInput, false);
  for (const event of [
    new MouseEvent("pointermove", { movementX: 0, movementY: 0 }),
    new MouseEvent("pointermove", { movementX: 4, movementY: 2 }),
    new MouseEvent("mousemove", { movementX: 4, movementY: 2 }),
    new MouseEvent("mouseenter"),
    new KeyboardEvent("keydown", { key: "j", repeat: true }),
    new KeyboardEvent("keyup", { key: "j" }),
    // What keeps a computer awake, a headset's buttons, a modifier alone.
    new KeyboardEvent("keydown", { key: "F15" }),
    new KeyboardEvent("keydown", { key: "ScrollLock" }),
    new KeyboardEvent("keydown", { key: "MediaPlayPause" }),
    new KeyboardEvent("keydown", { key: "AudioVolumeUp" }),
    new KeyboardEvent("keydown", { key: "Shift" }),
    new KeyboardEvent("keydown", { key: "Unidentified" }),
    new Event("pointerup"),
    new Event("focus"),
    new Event("scroll"),
  ]) {
    window.dispatchEvent(event);
  }
  expect(onInput).not.toHaveBeenCalled();
});

it("stops when told to", () => {
  const onInput = vi.fn();
  const onRelease = vi.fn();
  listenForStudentInput(onInput, false, onRelease)();
  window.dispatchEvent(new Event("pointerdown"));
  window.dispatchEvent(new MouseEvent("click"));
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "a" }));
  window.dispatchEvent(new Event("pointerup"));
  window.dispatchEvent(new KeyboardEvent("keyup", { key: "a" }));
  expect(onInput).not.toHaveBeenCalled();
  expect(onRelease).not.toHaveBeenCalled();
});

it("takes the keys a student uses the app with", () => {
  for (const key of ["a", "Enter", " ", "Tab", "ArrowDown", "Escape", "F5", "Process", "Dead"]) {
    expect(studentInput({ type: "keydown", isTrusted: true, key }, true), key).toBe("key");
  }
});

it("with trustedOnly, takes only the browser's own events", () => {
  const onInput = vi.fn();
  const onRelease = vi.fn();
  stop = listenForStudentInput(onInput, true, onRelease);
  // Whatever a script dispatches is not the student (and a test can dispatch nothing else).
  window.dispatchEvent(new Event("pointerdown"));
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "a" }));
  window.dispatchEvent(new MouseEvent("click"));
  window.dispatchEvent(new Event("pointerup"));
  window.dispatchEvent(new KeyboardEvent("keyup", { key: "a" }));
  expect(onInput).not.toHaveBeenCalled();
  expect(onRelease).not.toHaveBeenCalled();

  expect(studentInput({ type: "pointerdown", isTrusted: true }, true)).toBe("press");
  expect(studentInput({ type: "click", isTrusted: true }, true)).toBe("click");
  expect(studentInput({ type: "keydown", isTrusted: true, repeat: false }, true)).toBe("key");
  expect(studentInput({ type: "keydown", isTrusted: true, repeat: true }, true)).toBeNull();
  expect(studentInput({ type: "click", isTrusted: false }, true)).toBeNull();
});
