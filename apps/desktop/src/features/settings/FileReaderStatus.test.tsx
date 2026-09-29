import { screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { createMockApi } from "@/api/mock";
import { renderRoute } from "@/test/render";

async function helpSection() {
  const heading = await screen.findByRole("heading", { level: 2, name: "Help & feedback" });
  const region = heading.closest("section");
  if (!region) throw new Error("no Help & feedback section");
  return region;
}

describe("file reader status", () => {
  it("warns when the file reader can't start and counts what couldn't be read", async () => {
    renderRoute("/settings", { scenario: "worker-blocked" });
    const help = await helpSection();
    expect(
      await within(help).findByText(/file reader couldn't start \(often blocked by antivirus/),
    ).toBeInTheDocument();
    expect(
      within(help).getByText(
        "Files PageLamp couldn't read: 3 waiting for the reader to start · 1 took too long",
      ),
    ).toBeInTheDocument();
  });

  it("says nothing while the reader works", async () => {
    const api = createMockApi({ latencyMs: 0, syncStepMs: 0 });
    const doctor = vi.spyOn(api, "doctor");
    renderRoute("/settings", { api });
    const help = await helpSection();
    await vi.waitFor(() => expect(doctor).toHaveBeenCalled());
    expect(within(help).queryByText(/file reader/)).toBeNull();
    expect(within(help).queryByText(/couldn't read/)).toBeNull();
  });

  it("asks for a reinstall when the reader is from another version", async () => {
    const api = createMockApi({ latencyMs: 0, syncStepMs: 0 });
    const real = await api.doctor();
    api.doctor = async () => ({
      ...real,
      extract_worker: { status: "protocol_mismatch", spawn_ms: null },
    });
    renderRoute("/settings", { api });
    const help = await helpSection();
    expect(
      await within(help).findByText(/file reader is from a different version\. Reinstall PageLamp/),
    ).toBeInTheDocument();
  });
});
