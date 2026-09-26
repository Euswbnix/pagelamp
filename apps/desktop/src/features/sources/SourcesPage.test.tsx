import { configure, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { ApiError } from "@/api/errors";
import { createMockApi } from "@/api/mock";
import type { SyncEvent } from "@/api/types";
import { useUiStore } from "@/stores/ui";
import { renderRoute } from "@/test/render";

// The mock adds a fixed 500 ms to token/feed validation; leave headroom on slow CI machines.
// (Per-file setting: Vitest isolates each test file.)
configure({ asyncUtilTimeout: 3000 });

// Synthetic values only. The mock accepts tokens of 8+ characters without "expired".
const NEW_TOKEN = "demo-replacement-token-7f3a91";

function storageDump(): string {
  const entries: string[] = [];
  for (let i = 0; i < localStorage.length; i++) {
    const key = localStorage.key(i) ?? "";
    entries.push(`${key}=${localStorage.getItem(key)}`);
  }
  return entries.join("\n");
}

describe("SourcesPage", () => {
  it("lists the demo sources with their kind, settings and status", async () => {
    renderRoute("/sources");

    expect(
      await screen.findByRole("heading", { level: 1, name: "Sources & sync" }),
    ).toBeInTheDocument();
    expect(screen.getAllByRole("heading", { level: 1 })).toHaveLength(1);
    await waitFor(() =>
      expect(screen.getAllByRole("heading", { level: 2 }).map((h) => h.textContent)).toEqual([
        "Course folder",
        "Course calendar",
        "Demo Canvas",
      ]),
    );
    expect(screen.getByText("/Users/demo/Documents/Courses")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "https://canvas.demo.test" })).toBeInTheDocument();
    expect(screen.getByText("Private feed address (stored in your keychain)")).toBeInTheDocument();
    expect(screen.getAllByText("OK")).toHaveLength(3);
    expect(
      screen.getByRole("button", { name: "Replace token for Demo Canvas" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Replace feed address for Course calendar" }),
    ).toBeInTheDocument();
    // Folder sources have no secret to replace.
    expect(screen.queryByRole("button", { name: /Replace .* for Course folder/ })).toBeNull();
    expect(screen.getByText(/keeps your courses on this computer/)).toBeInTheDocument();
  });

  it("shows a skeleton while loading", async () => {
    renderRoute("/sources", { latencyMs: 20 });
    expect(screen.getByText("Loading your sources…")).toBeInTheDocument();
    expect(
      await screen.findByRole("heading", { level: 2, name: "Demo Canvas" }),
    ).toBeInTheDocument();
    expect(screen.queryByText("Loading your sources…")).not.toBeInTheDocument();
  });

  it("shows an error with a retry when the sources can't be loaded", async () => {
    const api = createMockApi({ latencyMs: 0, syncStepMs: 0 });
    const listSources = api.listSources;
    let fail = true;
    api.listSources = () =>
      fail ? Promise.reject(new ApiError("internal", "Database is locked")) : listSources();
    const { user } = renderRoute("/sources", { api });

    expect(await screen.findByText("Something went wrong")).toBeInTheDocument();
    expect(screen.getByText("Database is locked")).toBeInTheDocument();
    fail = false;
    await user.click(screen.getByRole("button", { name: "Try again" }));
    expect(
      await screen.findByRole("heading", { level: 2, name: "Demo Canvas" }),
    ).toBeInTheDocument();
  });

  it("shows the token-expired callout, and replacing the token clears the error", async () => {
    const { user, queryClient } = renderRoute("/sources", { scenario: "expired" });

    // The callout title and the card's status badge.
    expect(await screen.findAllByText("Access expired")).toHaveLength(2);
    expect(
      screen.getByText(
        "Student tokens last at most 30 days. Create a new token in Canvas (Account → Settings → New access token) and replace it here.",
      ),
    ).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Replace token for Demo Canvas" }));
    const dialog = await screen.findByRole("dialog", { name: "Replace token" });
    expect(
      within(dialog).getByText(
        "Personal access tokens are for your own use only; Canvas student tokens expire within 30 days.",
      ),
    ).toBeInTheDocument();
    await user.type(within(dialog).getByLabelText("New access token"), NEW_TOKEN);
    await user.click(within(dialog).getByRole("button", { name: "Replace" }));

    expect(await screen.findByText("Token replaced for Demo Canvas")).toBeInTheDocument();
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    await waitFor(() => expect(screen.queryByText("Access expired")).not.toBeInTheDocument());
    // The button that opened the dialog went away with the callout: focus lands on the h1.
    await waitFor(() =>
      expect(screen.getByRole("heading", { level: 1, name: "Sources & sync" })).toHaveFocus(),
    );

    // The token was handed to the backend and kept nowhere else.
    expect(document.body.innerHTML).not.toContain(NEW_TOKEN);
    expect(storageDump()).not.toContain(NEW_TOKEN);
    await waitFor(() =>
      expect(
        JSON.stringify(
          queryClient
            .getMutationCache()
            .getAll()
            .map((m) => m.state.variables),
        ),
      ).not.toContain(NEW_TOKEN),
    );
  });

  it("keeps the replace dialog open and explains a rejected token", async () => {
    const { user } = renderRoute("/sources", { scenario: "expired" });
    await user.click(await screen.findByRole("button", { name: "Replace token for Demo Canvas" }));
    const dialog = await screen.findByRole("dialog", { name: "Replace token" });

    await user.click(within(dialog).getByRole("button", { name: "Replace" }));
    expect(within(dialog).getByText("Paste the new token first.")).toBeInTheDocument();

    await user.type(within(dialog).getByLabelText("New access token"), "demo-token-expired-01");
    await user.click(within(dialog).getByRole("button", { name: "Replace" }));
    expect(
      await within(dialog).findByText(
        "Canvas didn't accept this token. It may have expired or been revoked — create a new one and try again.",
      ),
    ).toBeInTheDocument();
    expect(screen.getByRole("dialog")).toBeInTheDocument();
  });

  it("describes other source errors by kind", async () => {
    renderRoute("/sources", { scenario: "error" });
    expect(await screen.findByText("The last sync didn't work")).toBeInTheDocument();
    expect(screen.getByText("Not found")).toBeInTheDocument();
    expect(
      screen.getByText("Folder /Users/demo/Documents/Courses was not found."),
    ).toBeInTheDocument();
  });

  it("removes a source after confirmation", async () => {
    const { user } = renderRoute("/sources");

    await user.click(await screen.findByRole("button", { name: "Remove Course calendar" }));
    const dialog = await screen.findByRole("alertdialog", { name: "Remove Course calendar?" });
    expect(within(dialog).getByText(/everything synced from it/)).toBeInTheDocument();
    expect(within(dialog).getByText(/its stored feed address/)).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Remove source" }));

    expect(await screen.findByText("Removed Course calendar")).toBeInTheDocument();
    await waitFor(() =>
      expect(
        screen.queryByRole("heading", { level: 2, name: "Course calendar" }),
      ).not.toBeInTheDocument(),
    );
    expect(screen.getAllByRole("heading", { level: 2 })).toHaveLength(2);
    // The Remove button went away with its card, so focus moves to the page heading.
    await waitFor(() =>
      expect(screen.getByRole("heading", { level: 1, name: "Sources & sync" })).toHaveFocus(),
    );
  });

  it("explains that removing a folder source leaves the files alone", async () => {
    const { user } = renderRoute("/sources");
    await user.click(await screen.findByRole("button", { name: "Remove Course folder" }));
    const dialog = await screen.findByRole("alertdialog", { name: "Remove Course folder?" });
    expect(within(dialog).getByText(/The files in your folder aren't touched/)).toBeInTheDocument();
    // A folder source has no token or feed address to delete.
    expect(within(dialog).queryByText(/token|feed address/)).toBeNull();
  });

  it("cancelling the remove dialog keeps the source", async () => {
    const { user } = renderRoute("/sources");
    await user.click(await screen.findByRole("button", { name: "Remove Demo Canvas" }));
    const dialog = await screen.findByRole("alertdialog");
    await user.click(within(dialog).getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument());
    expect(screen.getByRole("heading", { level: 2, name: "Demo Canvas" })).toBeInTheDocument();
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Remove Demo Canvas" })).toHaveFocus(),
    );
  });

  it("replaces a calendar feed address and keeps it nowhere else", async () => {
    const feedUrl = "https://calendar.demo.test/feeds/private-replacement-5d19.ics";
    const { user } = renderRoute("/sources");
    const opener = await screen.findByRole("button", {
      name: "Replace feed address for Course calendar",
    });
    await user.click(opener);
    const dialog = await screen.findByRole("dialog", { name: "Replace feed address" });
    // The Canvas personal-use notice belongs to Canvas tokens only.
    expect(within(dialog).queryByText(/Personal access tokens/)).toBeNull();

    await user.type(within(dialog).getByLabelText("New calendar feed address"), feedUrl);
    await user.click(within(dialog).getByRole("button", { name: "Replace" }));

    expect(
      await screen.findByText("Feed address replaced for Course calendar"),
    ).toBeInTheDocument();
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    // Opened from the card footer, which is still there: focus goes back to that button.
    await waitFor(() => expect(opener).toHaveFocus());
    expect(document.body.innerHTML).not.toContain("private-replacement-5d19");
    expect(storageDump()).not.toContain("private-replacement-5d19");
  });

  it("skips the disclosure in the dialog once it was acknowledged", async () => {
    useUiStore.setState({ aiDisclosureAcknowledgedAt: "2026-09-01T12:00:00.000Z" });
    const { user } = renderRoute("/sources");
    await user.click(await screen.findByRole("button", { name: "Add source" }));
    const dialog = await screen.findByRole("dialog", { name: "Add a source" });
    expect(
      within(dialog).queryByRole("checkbox", { name: "I understand" }),
    ).not.toBeInTheDocument();
    expect(
      within(dialog).getByRole("radio", { name: "Course folder + calendar feed" }),
    ).toBeChecked();
  });

  it("returns focus to Add source when the dialog is cancelled", async () => {
    const { user } = renderRoute("/sources");
    const add = await screen.findByRole("button", { name: "Add source" });
    await user.click(add);
    const dialog = await screen.findByRole("dialog", { name: "Add a source" });
    await user.click(within(dialog).getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    await waitFor(() => expect(add).toHaveFocus());
  });

  it("shows the empty state and adds a first source from the dialog", async () => {
    const { user } = renderRoute("/sources", { scenario: "empty" });

    expect(await screen.findByText("No sources yet")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Sync all" })).toHaveAttribute(
      "aria-disabled",
      "true",
    );
    await user.click(screen.getByRole("button", { name: "Add your first source" }));

    const dialog = await screen.findByRole("dialog", { name: "Add a source" });
    // Onboarding was skipped, so the AI disclosure comes first and gates the forms.
    expect(within(dialog).getByText(/sends them to your AI provider/)).toBeInTheDocument();
    expect(within(dialog).queryByRole("radio")).not.toBeInTheDocument();
    await user.click(within(dialog).getByRole("checkbox", { name: "I understand" }));
    expect(
      within(dialog).getByRole("radio", { name: "Course folder + calendar feed" }),
    ).toBeChecked();
    await user.click(within(dialog).getByRole("button", { name: "Choose folder…" }));
    expect(
      await within(dialog).findByDisplayValue("/Users/demo/Documents/Courses"),
    ).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Add source" }));

    expect(await screen.findByText("Source added. Syncing now…")).toBeInTheDocument();
    expect(await screen.findByRole("heading", { level: 2, name: "Courses" })).toBeInTheDocument();
    expect(
      await screen.findByRole("heading", { level: 2, name: "Sync finished" }),
    ).toBeInTheDocument();
    expect(screen.queryByText("No sources yet")).not.toBeInTheDocument();
  });

  it("shows live progress while syncing everything, then the result", async () => {
    const api = createMockApi({ latencyMs: 0, syncStepMs: 0 });
    const realSyncAll = api.syncAll;
    let release: () => void = () => {};
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    // Emit the first events, then hold the run until the test has looked at it.
    api.syncAll = async (req, onEvent: (event: SyncEvent) => void) => {
      onEvent({ type: "source_started", source_id: "folder:demo-courses", label: "Course folder" });
      onEvent({
        type: "progress",
        source_id: "folder:demo-courses",
        message: "Indexing materials (3/12)",
        current: 3,
        total: 12,
      });
      await gate;
      return realSyncAll(req, onEvent);
    };
    const { user } = renderRoute("/sources", { api });

    const syncAll = await screen.findByRole("button", { name: "Sync all" });
    await screen.findByRole("heading", { level: 2, name: "Demo Canvas" });
    await user.click(syncAll);

    expect(await screen.findByRole("heading", { level: 2, name: "Syncing…" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Sync all" })).toHaveAttribute(
      "aria-disabled",
      "true",
    );
    const bar = screen.getByRole("progressbar", { name: "Course folder progress" });
    expect(bar).toHaveAttribute("aria-valuenow", "25");
    expect(screen.getByText("Indexing materials (3/12)")).toBeInTheDocument();
    expect(screen.getByText("Syncing")).toBeInTheDocument(); // the folder card's badge
    // No removing while a sync runs, even for a source that hasn't started yet.
    expect(screen.getByRole("button", { name: "Remove Demo Canvas" })).toBeDisabled();

    release();
    expect(
      await screen.findByRole("heading", { level: 2, name: "Sync finished" }),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Sync all" })).toBeEnabled();
    expect(screen.queryByRole("progressbar")).not.toBeInTheDocument();

    // The folder source reported a skipped file: warnings are collapsed until asked for.
    await user.click(screen.getByRole("button", { name: "Warnings (1)" }));
    expect(await screen.findByText(/video files can't be read/)).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Hide sync results" }));
    expect(screen.queryByRole("heading", { name: "Sync finished" })).not.toBeInTheDocument();
  });

  it("syncs a single source", async () => {
    const { user } = renderRoute("/sources", { scenario: "expired" });
    await user.click(await screen.findByRole("button", { name: "Sync Course calendar" }));
    expect(
      await screen.findByRole("heading", { level: 2, name: "Sync finished" }),
    ).toBeInTheDocument();
    const panel = screen.getByRole("region", { name: "Sync finished" });
    expect(within(panel).getByText("Course calendar")).toBeInTheDocument();
    expect(within(panel).queryByText("Demo Canvas")).not.toBeInTheDocument();
  });

  it("reports an expired token found during a sync", async () => {
    const { user } = renderRoute("/sources", { scenario: "expired" });
    await user.click(await screen.findByRole("button", { name: "Sync Demo Canvas" }));
    expect(
      await screen.findByRole("heading", { level: 2, name: "Sync finished with problems" }),
    ).toBeInTheDocument();
    const panel = screen.getByRole("region", { name: "Sync finished with problems" });
    expect(within(panel).getByText("Access expired")).toBeInTheDocument();
    expect(within(panel).getByText(/Access expired. Replace the token/)).toBeInTheDocument();
  });

  it("explains when another process is already syncing", async () => {
    const { user } = renderRoute("/sources", { scenario: "busy" });

    expect(await screen.findByText("Another sync is running")).toBeInTheDocument();
    expect(
      screen.getByText(
        "A sync is already running (maybe from the command line). Try again when it finishes.",
      ),
    ).toBeInTheDocument();

    // Syncing can't start while another process holds the lock: the button stays focusable
    // (aria-disabled) and clicking it starts nothing.
    const syncAll = screen.getByRole("button", { name: "Sync all" });
    expect(syncAll).toHaveAttribute("aria-disabled", "true");
    await user.click(syncAll);
    expect(syncAll).toHaveFocus();
    expect(
      screen.queryByRole("heading", { level: 2, name: "Sync failed" }),
    ).not.toBeInTheDocument();
    expect(screen.getByText("Another sync is running")).toBeInTheDocument();
  });
});
