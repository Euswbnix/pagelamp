import { render, screen } from "@testing-library/react";
import { Info } from "lucide-react";
import { describe, expect, it } from "vitest";
import { Alert, AlertAction, AlertDescription } from "./alert";
import { Button } from "./button";

// jsdom has no layout, so pin what does it (QA N1: at 1100 px "Set term dates" covered the
// note's last line when the action was positioned over a fixed right padding).
describe("Alert with an action", () => {
  it("puts the action in its own last column instead of over the text", () => {
    render(
      <Alert role="status">
        <Info aria-hidden />
        <AlertDescription>A note long enough to reach the action.</AlertDescription>
        <AlertAction>
          <Button size="xs">Set term dates</Button>
        </AlertAction>
      </Alert>,
    );
    const alert = screen.getByRole("status");
    expect(alert.className).toContain(
      "has-[>svg]:has-data-[slot=alert-action]:grid-cols-[auto_1fr_auto]",
    );
    expect(alert.className).toContain("has-data-[slot=alert-action]:grid-cols-[1fr_auto]");
    expect(alert.className).not.toMatch(/pr-18/);
    const action = screen.getByRole("button", { name: "Set term dates" }).parentElement;
    expect(action).toHaveAttribute("data-slot", "alert-action");
    expect(action).toHaveClass("col-end-[-1]", "row-start-1");
    expect(action).not.toHaveClass("absolute");
  });
});
