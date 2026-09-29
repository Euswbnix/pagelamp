import { describe, expect, it } from "vitest";
import { applyPreferences } from "./appearance";

describe("applyPreferences", () => {
  it("puts the theme, transparency and contrast on <html> before the first render", () => {
    const root = document.createElement("html");
    applyPreferences(root, { theme: "system", transparency: "reduced", contrast: "more" }, true);
    expect(root.classList.contains("dark")).toBe(true);
    expect(root.style.colorScheme).toBe("dark");
    expect(root.dataset.transparency).toBe("reduced");
    expect(root.dataset.contrast).toBe("more");

    applyPreferences(root, { theme: "light", transparency: "auto", contrast: "auto" }, true);
    expect(root.classList.contains("dark")).toBe(false);
    expect(root.dataset.transparency).toBe("auto");
  });
});
