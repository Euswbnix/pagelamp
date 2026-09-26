import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { createMockApi } from "@/api/mock";
import { Providers } from "@/app/Providers";
import { QuarantineHint, quarantineTarget } from "./QuarantineHint";

function renderHint(command: string) {
  render(
    <Providers api={createMockApi({ latencyMs: 0 })}>
      <QuarantineHint command={command} />
    </Providers>,
  );
}

describe("QuarantineHint", () => {
  it("leads with macOS's Open Anyway path and offers xattr on the installed app", () => {
    renderHint("/Applications/PageLamp.app/Contents/MacOS/pagelamp");
    expect(screen.getByText(/click “Open Anyway”\./)).toBeInTheDocument();
    expect(screen.getByText("Still blocked? Run this once in Terminal:")).toBeInTheDocument();
    expect(
      screen.getByText("xattr -dr com.apple.quarantine '/Applications/PageLamp.app'"),
    ).toBeInTheDocument();
  });

  it("uses wherever the app actually is, quoted for the shell", () => {
    renderHint("/Users/demo/My Apps/Page'Lamp.app/Contents/MacOS/pagelamp");
    expect(
      screen.getByText(`xattr -dr com.apple.quarantine '/Users/demo/My Apps/Page'\\''Lamp.app'`),
    ).toBeInTheDocument();
  });

  it("falls back to the binary itself outside an app bundle", () => {
    expect(quarantineTarget("/Users/demo/bin/pagelamp")).toEqual({
      path: "/Users/demo/bin/pagelamp",
      recursive: false,
    });
    expect(quarantineTarget("/Applications/PageLamp.app/Contents/MacOS/pagelamp")).toEqual({
      path: "/Applications/PageLamp.app",
      recursive: true,
    });
  });
});
