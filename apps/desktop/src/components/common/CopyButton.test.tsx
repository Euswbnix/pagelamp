import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { CopyButton } from "./CopyButton";

describe("CopyButton", () => {
  it("copies and announces it once from a status outside the button", async () => {
    const user = userEvent.setup();
    render(<CopyButton text="claude mcp add studentos" label="Copy command" />);
    const button = screen.getByRole("button", { name: "Copy command" });
    expect(screen.getByRole("status")).toBeEmptyDOMElement();

    await user.click(button);
    expect(await navigator.clipboard.readText()).toBe("claude mcp add studentos");
    expect(screen.getByRole("status")).toHaveTextContent("Copied");
    expect(button).not.toContainElement(screen.getByRole("status"));
  });
});
