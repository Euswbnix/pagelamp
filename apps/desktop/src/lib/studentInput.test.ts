import { afterEach, expect, it, vi } from "vitest";
import { isStudentInput, listenForStudentInput } from "./studentInput";

let stop: (() => void) | null = null;
afterEach(() => {
  stop?.();
  stop = null;
  document.body.replaceChildren();
});

it("calls back for a press, a click, a key going down and the wheel, wherever they happen", () => {
  const onInput = vi.fn();
  stop = listenForStudentInput(onInput, false);
  const button = document.body.appendChild(document.createElement("button"));
  // Something on the page that keeps the event to itself doesn't hide it.
  button.addEventListener("pointerdown", (event) => event.stopPropagation());

  button.dispatchEvent(new Event("pointerdown", { bubbles: true }));
  expect(onInput).toHaveBeenCalledTimes(1);
  button.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  expect(onInput).toHaveBeenCalledTimes(2);
  window.dispatchEvent(new WheelEvent("wheel", { deltaY: 40 }));
  expect(onInput).toHaveBeenCalledTimes(3);
  // A control activated without a press before it (assistive technology).
  button.dispatchEvent(new MouseEvent("click", { bubbles: true }));
  expect(onInput).toHaveBeenCalledTimes(4);
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
  listenForStudentInput(onInput, false)();
  window.dispatchEvent(new Event("pointerdown"));
  window.dispatchEvent(new MouseEvent("click"));
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "a" }));
  window.dispatchEvent(new WheelEvent("wheel"));
  expect(onInput).not.toHaveBeenCalled();
});

it("takes the keys a student uses the app with", () => {
  for (const key of ["a", "Enter", " ", "Tab", "ArrowDown", "Escape", "F5", "Process", "Dead"]) {
    expect(isStudentInput({ type: "keydown", isTrusted: true, key }, true), key).toBe(true);
  }
});

it("with trustedOnly, takes only the browser's own events", () => {
  const onInput = vi.fn();
  stop = listenForStudentInput(onInput, true);
  // Whatever a script dispatches is not the student (and a test can dispatch nothing else).
  window.dispatchEvent(new Event("pointerdown"));
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "a" }));
  window.dispatchEvent(new WheelEvent("wheel"));
  expect(onInput).not.toHaveBeenCalled();

  expect(isStudentInput({ type: "pointerdown", isTrusted: true }, true)).toBe(true);
  expect(isStudentInput({ type: "keydown", isTrusted: true, repeat: false }, true)).toBe(true);
  expect(isStudentInput({ type: "keydown", isTrusted: true, repeat: true }, true)).toBe(false);
  expect(isStudentInput({ type: "wheel", isTrusted: false }, true)).toBe(false);
});
