import { screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { useUiStore } from "@/stores/ui";
import { renderRoute } from "@/test/render";

afterEach(() => {
  delete document.documentElement.dataset.platform;
});

describe("Settings ▸ Appearance: transparency and contrast", () => {
  it("reduces transparency on <html> and remembers it", async () => {
    const { user } = renderRoute("/settings");
    const toggle = await screen.findByRole("switch", { name: "Reduce transparency" });
    expect(document.documentElement.dataset.transparency).toBe("auto");
    await user.click(toggle);
    expect(toggle).toBeChecked();
    expect(useUiStore.getState().transparency).toBe("reduced");
    expect(document.documentElement.dataset.transparency).toBe("reduced");
    // Increase contrast is for Linux only (elsewhere the system setting reaches the web view).
    expect(screen.queryByRole("switch", { name: "Increase contrast" })).toBeNull();
  });

  it("offers Increase contrast on Linux", async () => {
    document.documentElement.dataset.platform = "linux";
    const { user } = renderRoute("/settings");
    const toggle = await screen.findByRole("switch", { name: "Increase contrast" });
    await user.click(toggle);
    expect(document.documentElement.dataset.contrast).toBe("more");
  });
});
