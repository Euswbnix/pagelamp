import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { createMockApi } from "@/api/mock";
import { Providers } from "@/app/Providers";
import { brand } from "@/brand";
import { QuarantineHint } from "./QuarantineHint";

describe("QuarantineHint", () => {
  it("leads with macOS's Open Anyway path and offers xattr as the fallback", () => {
    render(
      <Providers api={createMockApi({ latencyMs: 0 })}>
        <QuarantineHint />
      </Providers>,
    );
    expect(screen.getByText(/click “Open Anyway”\./)).toBeInTheDocument();
    expect(screen.getByText("Still blocked? Run this once in Terminal:")).toBeInTheDocument();
    expect(
      screen.getByText(`xattr -dr com.apple.quarantine "/Applications/${brand.productName}.app"`),
    ).toBeInTheDocument();
  });
});
