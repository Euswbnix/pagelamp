import { act, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ApiError } from "@/api/errors";
import { createMockApi } from "@/api/mock";
import { MOCK_APP_VERSION, MOCK_UPDATE_VERSION } from "@/api/mock/fixtures";
import { useSyncStore } from "@/stores/sync";
import { renderRoute } from "@/test/render";

function mockApi(options: Parameters<typeof createMockApi>[0] = {}) {
  return createMockApi({ latencyMs: 0, syncStepMs: 0, ...options });
}

async function updatesSection() {
  const heading = await screen.findByRole("heading", { level: 2, name: "Updates" });
  const region = heading.closest("section");
  if (!region) throw new Error("no Updates section");
  return region;
}

describe("onboarding", () => {
  it("discloses the update check with a switch that's on, and records the choice", async () => {
    const api = mockApi({ scenario: "empty" });
    const setPrefs = vi.spyOn(api, "setUpdatePrefs");
    const acknowledge = vi.spyOn(api, "acknowledgeUpdateDisclosure");
    const { user } = renderRoute("/welcome", { api });

    const toggle = await screen.findByRole("switch", { name: "Check for updates automatically" });
    expect(toggle).toBeChecked();
    expect(toggle).toHaveAccessibleDescription(/only your IP address is sent/);
    await user.click(toggle);
    await user.click(screen.getByRole("checkbox", { name: "I understand" }));
    await user.click(screen.getByRole("button", { name: "Get started" }));

    await waitFor(() => expect(acknowledge).toHaveBeenCalledTimes(1));
    expect(setPrefs).toHaveBeenCalledWith({ auto_check: false, channel: null });
  });

  it("records the disclosure when the student skips too", async () => {
    const api = mockApi({ scenario: "empty" });
    const acknowledge = vi.spyOn(api, "acknowledgeUpdateDisclosure");
    const setPrefs = vi.spyOn(api, "setUpdatePrefs");
    const { user } = renderRoute("/welcome", { api });
    await user.click(await screen.findByRole("button", { name: "Skip for now" }));
    await waitFor(() => expect(acknowledge).toHaveBeenCalledTimes(1));
    // Left on: nothing to change.
    expect(setPrefs).not.toHaveBeenCalled();
  });
});

describe("What's new (upgraders)", () => {
  it("explains the update check before the first automatic check, then runs it", async () => {
    const api = mockApi({ scenario: "upgrader" });
    const check = vi.spyOn(api, "checkForUpdate");
    const acknowledge = vi.spyOn(api, "acknowledgeWhatsNew");
    const { user } = renderRoute("/courses", { api });

    const sheet = await screen.findByRole("dialog", { name: "What's new in PageLamp" });
    expect(within(sheet).getByText("Since version 0.1.0")).toBeInTheDocument();
    expect(within(sheet).getByText("PageLamp now updates itself")).toBeInTheDocument();
    expect(within(sheet).getByText("Weeks and phases for every course")).toBeInTheDocument();
    expect(
      within(sheet).getByRole("switch", { name: "Check for updates automatically" }),
    ).toBeChecked();
    // Nothing is checked while the student is still reading about it.
    expect(check).not.toHaveBeenCalled();

    await user.click(within(sheet).getByRole("button", { name: "Got it" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(acknowledge).toHaveBeenCalledTimes(1);
    await waitFor(() => expect(check).toHaveBeenCalledTimes(1));
  });

  it("never checks if the student turns it off there", async () => {
    const api = mockApi({ scenario: "upgrader" });
    const check = vi.spyOn(api, "checkForUpdate");
    const setPrefs = vi.spyOn(api, "setUpdatePrefs");
    const { user } = renderRoute("/courses", { api });
    const sheet = await screen.findByRole("dialog", { name: "What's new in PageLamp" });
    await user.click(
      within(sheet).getByRole("switch", { name: "Check for updates automatically" }),
    );
    await user.click(within(sheet).getByRole("button", { name: "Got it" }));
    await waitFor(() =>
      expect(setPrefs).toHaveBeenCalledWith({ auto_check: false, channel: null }),
    );
    // The facade no longer reports a check as due.
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(check).not.toHaveBeenCalled();
  });

  it("isn't shown to anyone else", async () => {
    const api = mockApi();
    const tasks = vi.spyOn(api, "startupTasks");
    renderRoute("/courses", { api });
    await waitFor(() => expect(tasks).toHaveBeenCalled());
    expect(screen.queryByRole("dialog", { name: "What's new in PageLamp" })).toBeNull();
  });
});

describe("installing an update", () => {
  it("offers the update, asks first, then installs with progress", async () => {
    const api = mockApi({ scenario: "update-available" });
    const install = vi.spyOn(api, "installUpdate");
    const { user } = renderRoute("/courses", { api });

    const notice = await screen.findByRole("region", {
      name: `PageLamp ${MOCK_UPDATE_VERSION} is available.`,
    });
    expect(install).not.toHaveBeenCalled();
    await user.click(within(notice).getByRole("button", { name: "Install…" }));

    const dialog = await screen.findByRole("alertdialog", {
      name: `Install PageLamp ${MOCK_UPDATE_VERSION}?`,
    });
    expect(within(dialog).getByText(/quit and reopen your AI app/)).toBeInTheDocument();
    expect(install).not.toHaveBeenCalled();
    await user.click(within(dialog).getByRole("button", { name: "Install and restart" }));

    expect(await within(dialog).findByText("Restarting PageLamp…")).toBeInTheDocument();
    expect(install).toHaveBeenCalledTimes(1);
    // Mid-install it can't be cancelled.
    expect(within(dialog).getByRole("button", { name: "Cancel" })).toBeDisabled();
  });

  it("waits while a sync runs, and says so", async () => {
    const api = mockApi({ scenario: "update-available" });
    const install = vi.spyOn(api, "installUpdate");
    const { user } = renderRoute("/courses", { api });
    const notice = await screen.findByRole("region", { name: /is available\./ });
    await user.click(within(notice).getByRole("button", { name: "Install…" }));
    const dialog = await screen.findByRole("alertdialog");

    act(() => useSyncStore.setState({ running: true }));
    const button = within(dialog).getByRole("button", { name: "Install and restart" });
    expect(button).toHaveAttribute("aria-disabled", "true");
    expect(button).toHaveAccessibleDescription("Available when the sync finishes");
    await user.click(button);
    expect(install).not.toHaveBeenCalled();
  });

  it("hides the notice for this launch with Later", async () => {
    const { user } = renderRoute("/courses", { scenario: "update-available" });
    const notice = await screen.findByRole("region", { name: /is available\./ });
    await user.click(within(notice).getByRole("button", { name: "Later" }));
    expect(screen.queryByRole("region", { name: /is available\./ })).toBeNull();
  });

  it("links deb/rpm installs to the download instead", async () => {
    const api = mockApi({ scenario: "deb" });
    const openExternal = vi.spyOn(api, "openExternal");
    const { user } = renderRoute("/courses", { api });
    const notice = await screen.findByRole("region", { name: /is available\./ });
    expect(within(notice).queryByRole("button", { name: "Install…" })).toBeNull();
    await user.click(within(notice).getByRole("button", { name: "Download" }));
    expect(openExternal).toHaveBeenCalledWith(
      `https://github.com/Euswbnix/pagelamp/releases/tag/v${MOCK_UPDATE_VERSION}`,
    );
  });
});

describe("after an update", () => {
  it("asks the student to reopen their AI app, until dismissed", async () => {
    const { user } = renderRoute("/courses", { scenario: "updated" });
    const banner = await screen.findByRole("region", {
      name: `PageLamp was updated to ${MOCK_APP_VERSION}`,
    });
    expect(within(banner).getByText(/Quit and reopen your AI app/)).toBeInTheDocument();
    await user.click(within(banner).getByRole("button", { name: "Dismiss" }));
    expect(screen.queryByRole("region", { name: /was updated to/ })).toBeNull();
  });
});

describe("Settings → Updates", () => {
  it("shows the version and preferences, and checks on request", async () => {
    const api = mockApi();
    const setPrefs = vi.spyOn(api, "setUpdatePrefs");
    const { user } = renderRoute("/settings", { api });
    const section = await updatesSection();

    expect(await within(section).findByText(MOCK_APP_VERSION)).toBeInTheDocument();
    expect(await within(section).findByText(/^Last checked/)).toBeInTheDocument();
    // A pre-release build defaults to the beta channel.
    expect(within(section).getByRole("radio", { name: "Beta" })).toBeChecked();
    await user.click(within(section).getByRole("radio", { name: "Stable" }));
    expect(setPrefs).toHaveBeenCalledWith({ auto_check: true, channel: "stable" });

    await user.click(within(section).getByRole("button", { name: "Check now" }));
    expect(await within(section).findByText("PageLamp is up to date.")).toBeInTheDocument();
  });

  it("offers the update it finds, with its release notes", async () => {
    const { user } = renderRoute("/settings", { scenario: "update-available" });
    const section = await updatesSection();
    // The launch check already found it (no check in the last day).
    expect(
      await within(section).findByText(`PageLamp ${MOCK_UPDATE_VERSION} is available.`),
    ).toBeInTheDocument();
    await user.click(within(section).getByText("Release notes"));
    expect(within(section).getByText(/Finished courses move to a “Past” group/)).toBeVisible();
    expect(within(section).getByRole("button", { name: "Install and restart…" })).toBeVisible();
  });

  it("says when a check fails", async () => {
    const api = mockApi();
    vi.spyOn(api, "checkForUpdate").mockRejectedValue(new ApiError("network", "offline"));
    const { user } = renderRoute("/settings", { api });
    const section = await updatesSection();
    await user.click(within(section).getByRole("button", { name: "Check now" }));
    expect(await within(section).findByText("Couldn't check for updates.")).toBeInTheDocument();
  });
});
