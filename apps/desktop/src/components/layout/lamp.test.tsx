import { act, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { renderRoute } from "@/test/render";

const pool = () => document.querySelector(".pl-lamp");

describe("lamp band", () => {
  it("lights on the home screen, beside its title, and goes out elsewhere", async () => {
    const { router } = renderRoute("/courses");
    const heading = await screen.findByRole("heading", { level: 1, name: "Courses" });
    expect(pool()).toHaveAttribute("data-lit");
    expect(pool()).toHaveAttribute("aria-hidden", "true");
    expect(heading.closest(".pl-lamp-text")).not.toBeNull();

    await act(() => router.navigate("/settings"));
    await screen.findByRole("heading", { level: 1, name: "Settings" });
    expect(pool()).not.toHaveAttribute("data-lit");
    expect(document.querySelector(".pl-lamp-text")).toBeNull();
  });
});
