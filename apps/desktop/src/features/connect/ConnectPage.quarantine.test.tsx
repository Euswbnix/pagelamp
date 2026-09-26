// The quarantine hint only renders in production macOS builds; pretend to be one.

import { screen } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { MOCK_BINARY_PATH } from "@/api/mock/fixtures";
import { renderRoute } from "@/test/render";

vi.mock("@/lib/platform", () => ({ MAC_QUARANTINE_HINT: true }));

it("shows the macOS quarantine hint once for the page, for the binary the AI apps launch", async () => {
  renderRoute("/connect");
  expect(await screen.findByRole("heading", { level: 2, name: "Claude Desktop" })).toBeVisible();
  expect(screen.getAllByText("If macOS blocks PageLamp")).toHaveLength(1);
  // The mock's binary is a dev build (no .app), so the flag is cleared on the binary itself.
  expect(
    screen.getByText(`xattr -d com.apple.quarantine '${MOCK_BINARY_PATH}'`),
  ).toBeInTheDocument();
});
