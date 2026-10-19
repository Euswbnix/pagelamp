import { screen, waitFor, within } from "@testing-library/react";
import type { UserEvent } from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { createMockApi } from "@/api/mock";
import { useSyncStore } from "@/stores/sync";
import { useUiStore } from "@/stores/ui";
import { useUpdateStore } from "@/stores/updates";
import { renderRoute } from "@/test/render";

async function aiSection() {
  const heading = await screen.findByRole("heading", { level: 2, name: "AI models" });
  const region = heading.closest("section");
  if (!region) throw new Error("no AI models section");
  return region;
}

afterEach(() => vi.restoreAllMocks());

/** Press Tab until `target` has focus (fails after a bounded number of stops). */
async function tabTo(user: UserEvent, target: HTMLElement) {
  for (let i = 0; i < 20 && document.activeElement !== target; i++) await user.tab();
  expect(target).toHaveFocus();
}

describe("Settings → AI models", () => {
  it("adds an API key, then asks to read the disclosure before turning it on", async () => {
    const { user } = renderRoute("/settings");
    const section = await aiSection();
    expect(await within(section).findByText(/No model set up yet/)).toBeInTheDocument();

    await user.click(within(section).getByRole("button", { name: "Add an API key" }));
    const dialog = await screen.findByRole("dialog", { name: "Add an API key" });
    // The preset's data-policy line shows before any key is typed.
    expect(within(dialog).getByText(/Sent to OpenAI · not used for training/)).toBeInTheDocument();
    await user.type(within(dialog).getByLabelText("API key"), "sk-demo-typed-key-7Qx2");
    await user.click(within(dialog).getByRole("button", { name: "Check and add" }));

    // Straight on to the disclosure sheet, with all of Canvas §2E's items.
    const sheet = await screen.findByRole("dialog", { name: "Before PageLamp uses OpenAI" });
    for (const heading of [
      "PageLamp uses generative AI",
      "What is sent, and to whom",
      "Training and storage",
      "Cost",
      "Age",
      "Limitations and risks",
      "Who owns the materials",
    ]) {
      expect(within(sheet).getByRole("heading", { name: heading })).toBeInTheDocument();
    }
    expect(within(sheet).getByText(/labelled AI-generated/)).toBeInTheDocument();
    expect(
      within(sheet).getByText("OpenAI doesn't use what PageLamp sends to train its models."),
    ).toBeInTheDocument();

    // The age confirmation (D16) comes first.
    const turnOn = within(sheet).getByRole("button", { name: "Turn on OpenAI" });
    expect(turnOn).toHaveAttribute("aria-disabled", "true");
    await user.click(turnOn);
    expect(screen.getByRole("dialog", { name: "Before PageLamp uses OpenAI" })).toBeInTheDocument();
    await user.click(within(sheet).getByLabelText("I meet OpenAI's age requirement."));
    await user.click(turnOn);

    const row = (await within(section).findByRole("heading", { level: 3, name: "OpenAI" })).closest(
      "li",
    );
    if (!row) throw new Error("no OpenAI row");
    expect(await within(row).findByText("Ready")).toBeInTheDocument();
    expect(within(row).getByText("Key ending in 7Qx2")).toBeInTheDocument();
  });

  it("works from the keyboard alone, and the sheet is read from the top", async () => {
    const { user } = renderRoute("/settings");
    const section = await aiSection();
    const add = await within(section).findByRole("button", { name: "Add an API key" });
    add.focus();
    await user.keyboard("{Enter}");
    const dialog = await screen.findByRole("dialog", { name: "Add an API key" });
    await tabTo(user, within(dialog).getByLabelText("API key"));
    await user.keyboard("sk-demo-keyboard-only-1234{Enter}");

    const sheet = await screen.findByRole("dialog", { name: "Before PageLamp uses OpenAI" });
    await waitFor(() => expect(sheet).toHaveFocus());
    const turnOn = within(sheet).getByRole("button", { name: "Turn on OpenAI" });
    expect(turnOn).toHaveAccessibleDescription("Tick the confirmation above to turn it on.");
    await tabTo(
      user,
      within(sheet).getByRole("checkbox", { name: "I meet OpenAI's age requirement." }),
    );
    await user.keyboard(" ");
    await tabTo(user, turnOn);
    await user.keyboard("{Enter}");
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    // Focus lands on the new backend's row, which now says it's ready.
    const row = within(section).getByRole("heading", { level: 3, name: "OpenAI" }).closest("li");
    await waitFor(() => expect(row).toHaveFocus());
    expect(within(row as HTMLElement).getByText("Ready")).toBeInTheDocument();
  });

  it("never keeps the key in query keys or caches, stores, storage, logs or the page", async () => {
    const logs = (["log", "info", "warn", "error", "debug"] as const).map((level) =>
      vi.spyOn(console, level),
    );
    const { user, queryClient } = renderRoute("/settings");
    const section = await aiSection();
    const key = "sk-demo-VERY-SECRET-kx93";

    // A rejected key first (the failure path), then an accepted one.
    await user.click(await within(section).findByRole("button", { name: "Add an API key" }));
    let dialog = await screen.findByRole("dialog", { name: "Add an API key" });
    await user.type(within(dialog).getByLabelText("API key"), "sk-bad-SECRET-rejected");
    await user.click(within(dialog).getByRole("button", { name: "Check and add" }));
    expect(await within(dialog).findByText(/The provider rejected the key/)).toBeInTheDocument();
    await user.keyboard("{Escape}");

    await user.click(within(section).getByRole("button", { name: "Add an API key" }));
    dialog = await screen.findByRole("dialog", { name: "Add an API key" });
    await user.type(within(dialog).getByLabelText("API key"), key);
    await user.click(within(dialog).getByRole("button", { name: "Check and add" }));
    await screen.findByRole("dialog", { name: "Before PageLamp uses OpenAI" });

    const everything = JSON.stringify({
      queryKeys: queryClient
        .getQueryCache()
        .getAll()
        .map((q) => q.queryKey),
      queryData: queryClient
        .getQueryCache()
        .getAll()
        .map((q) => q.state.data),
      mutations: queryClient
        .getMutationCache()
        .getAll()
        .map((m) => m.state),
      stores: [useUiStore.getState(), useSyncStore.getState(), useUpdateStore.getState()],
      localStorage: { ...localStorage },
      sessionStorage: { ...sessionStorage },
      logs: logs.map((spy) => spy.mock.calls),
      page: document.body.innerHTML,
    });
    for (const secret of ["VERY-SECRET", "SECRET-rejected"]) {
      expect(everything).not.toContain(secret);
    }
    expect(everything).toContain("kx93"); // the last 4 characters are the only part kept
  });

  it("refuses a coding-plan key and quotes the provider's terms", async () => {
    const { user } = renderRoute("/settings");
    const section = await aiSection();
    await user.click(await within(section).findByRole("button", { name: "Add an API key" }));
    const dialog = await screen.findByRole("dialog", { name: "Add an API key" });
    await user.type(within(dialog).getByLabelText("API key"), "sk-sp-demo-coding");
    await user.click(within(dialog).getByRole("button", { name: "Check and add" }));
    expect(await within(dialog).findByText(/it's from a coding plan/)).toBeInTheDocument();
    expect(
      within(dialog).getByText(/only be used in the vendor's own coding tools/),
    ).toBeInTheDocument();
  });

  it("adds a custom server on this computer without a key, and asks one of any other", async () => {
    const { user, api } = renderRoute("/settings");
    const add = vi.spyOn(api, "addModelProvider");
    const section = await aiSection();
    await user.click(await within(section).findByRole("button", { name: "Add an API key" }));
    const dialog = await screen.findByRole("dialog", { name: "Add an API key" });
    await user.click(within(dialog).getByRole("combobox", { name: "Provider" }));
    await user.click(await screen.findByRole("option", { name: "Custom (OpenAI-compatible)" }));
    const address = within(dialog).getByLabelText("Address");
    // Elsewhere, the key is needed.
    await user.type(address, "https://llm.example.edu/v1");
    await user.click(within(dialog).getByRole("button", { name: "Check and add" }));
    expect(within(dialog).getByLabelText("API key")).toHaveAttribute("aria-invalid", "true");
    expect(add).not.toHaveBeenCalled();
    // On this computer, it may be left out.
    await user.clear(address);
    await user.type(address, "http://localhost:8080/v1");
    const key = within(dialog).getByLabelText("API key (optional)");
    expect(key).toHaveAccessibleDescription(/may not need a key/);
    // The key flagged as missing before isn't a mistake any more.
    expect(key).not.toHaveAttribute("aria-invalid");
    expect(within(dialog).queryByText("Please check what you entered.")).toBeNull();
    await user.click(within(dialog).getByRole("button", { name: "Check and add" }));
    expect(add).toHaveBeenCalledWith("custom", "http://localhost:8080/v1", null);
    // Its models may run anywhere: the note to read is the one of its cloud models.
    const sheet = await screen.findByRole("dialog", {
      name: "Before PageLamp uses cloud models through localhost:8080",
    });
    expect(
      within(sheet).getByText(
        /^localhost:8080 runs on this computer, but PageLamp can't confirm that each of its models does/,
      ),
    ).toBeInTheDocument();
    expect(
      within(sheet).getByText(/^A model may run in the cloud through .+, not on this computer/),
    ).toBeInTheDocument();
    await user.click(
      within(sheet).getByRole("button", { name: "Allow cloud models through localhost:8080" }),
    );
    // Set up without a key, and nothing says one is missing.
    const row = (await within(section).findByRole("heading", { name: "localhost:8080" })).closest(
      "li",
    );
    if (!row) throw new Error("no row");
    expect(within(row).queryByText(/key is missing/)).toBeNull();
    expect(within(row).getByText("Ready")).toBeInTheDocument();
    // Only the note of its cloud models: nothing says what it gets stays here.
    expect(
      within(row).getByRole("button", { name: "What its cloud models get" }),
    ).toBeInTheDocument();
    expect(within(row).queryByRole("button", { name: "What's shared" })).toBeNull();
    expect(within(row).queryByRole("button", { name: "Review and turn on" })).toBeNull();
    expect(within(row).queryByText(/Stays on this computer|doesn't leave/)).toBeNull();
    expect(within(row).getByText(/may run in the cloud/)).toBeInTheDocument();
    // A key can still be given later.
    await user.click(within(row).getByRole("button", { name: "Add a key" }));
    expect(
      await screen.findByRole("dialog", { name: "Add a key for localhost:8080" }),
    ).toBeInTheDocument();
  });

  it("shows a server's note of its cloud models apart from its own, and turns them on", async () => {
    // Ollama's own note is read; one feature now uses a model it runs in the cloud.
    const mock = createMockApi({ latencyMs: 0, syncStepMs: 0, scenario: "ai-local" });
    await mock.setFeatureModel("weekly_explanation", {
      backend: { kind: "provider", provider_id: "ollama" },
      model: "gpt-oss:120b-cloud",
      effort: "lowest",
    });
    const { user, api } = renderRoute("/settings", { api: mock });
    const read = vi.spyOn(api, "acknowledgeModelDisclosure");
    const section = await aiSection();
    const row = (await within(section).findByRole("heading", { name: "Ollama" })).closest("li");
    if (!row) throw new Error("no Ollama row");
    // The model picked may run in the cloud: said so, never as a fact.
    expect(
      await within(section).findByText(
        "gpt-oss:120b-cloud may run in the cloud, not on this computer.",
      ),
    ).toBeInTheDocument();
    // Its own note stays as read; the cloud models' one is asked for, as the step to take.
    expect(within(row).getByRole("button", { name: "What's shared" })).toHaveAttribute(
      "data-variant",
      "outline",
    );
    const allow = within(row).getByRole("button", { name: "Allow its cloud models" });
    expect(allow).toHaveAttribute("data-variant", "default");
    // A server that takes no key gets no key button.
    expect(within(row).queryByRole("button", { name: /key/ })).toBeNull();
    await user.click(allow);
    const sheet = await screen.findByRole("dialog", {
      name: "Before PageLamp uses cloud models through Ollama",
    });
    expect(
      within(sheet).getByText(/^A model may run in the cloud through Ollama/),
    ).toBeInTheDocument();
    expect(within(sheet).queryByText(/this model/)).toBeNull();
    await user.click(
      within(sheet).getByRole("button", { name: "Allow cloud models through Ollama" }),
    );
    expect(read).toHaveBeenCalledWith(
      { kind: "provider", provider_id: "ollama" },
      false,
      expect.any(Number),
    );
    expect(
      await within(row).findByRole("button", { name: "What its cloud models get" }),
    ).toBeInTheDocument();
  });

  it("adds a running Ollama in one click; nothing leaves this computer", async () => {
    const { user } = renderRoute("/settings");
    const section = await aiSection();
    expect(await within(section).findByText(/Not running\. Start it/)).toBeInTheDocument();
    await user.click(await within(section).findByRole("button", { name: "Use Ollama" }));
    const sheet = await screen.findByRole("dialog", { name: "Before PageLamp uses Ollama" });
    expect(
      within(sheet).getByText("It goes to Ollama on this computer and doesn't leave it."),
    ).toBeInTheDocument();
    expect(within(sheet).queryByRole("heading", { name: "Age" })).toBeNull();
    await user.click(within(sheet).getByRole("button", { name: "Turn on Ollama" }));
    expect(await within(section).findByText("Added")).toBeInTheDocument();
    expect(within(section).getByText("Stays on this computer · free")).toBeInTheDocument();
  });

  it("asks again when what a service receives changed", async () => {
    const { user } = renderRoute("/settings", { scenario: "ai-disclosure-changed" });
    const section = await aiSection();
    expect(await within(section).findByText("Not turned on yet")).toBeInTheDocument();
    expect(within(section).getByText(/has changed since you last read it/)).toBeInTheDocument();
    await user.click(within(section).getByRole("button", { name: "Review and turn on" }));
    const sheet = await screen.findByRole("dialog", { name: "Before PageLamp uses Anthropic" });
    expect(within(sheet).getByText("This changed since you last read it.")).toBeInTheDocument();
  });

  it("says which note changed: the one of a server's cloud models apart from its own", async () => {
    const mock = createMockApi({ latencyMs: 0, syncStepMs: 0, scenario: "ai-local" });
    const status = mock.aiStatus;
    // Its cloud models' note was accepted in an older version; its own is current.
    vi.spyOn(mock, "aiStatus").mockImplementation(async () => {
      const s = await status();
      return {
        ...s,
        backends: s.backends.map((b) =>
          b.label === "Ollama" && b.cloud_disclosure
            ? { ...b, cloud_disclosure_acknowledged: b.cloud_disclosure.version - 1 }
            : b,
        ),
      };
    });
    const { user } = renderRoute("/settings", { api: mock });
    const section = await aiSection();
    const row = (await within(section).findByRole("heading", { name: "Ollama" })).closest("li");
    if (!row) throw new Error("no Ollama row");
    await user.click(within(row).getByRole("button", { name: "What's shared" }));
    const own = await screen.findByRole("dialog", { name: "Before PageLamp uses Ollama" });
    expect(within(own).queryByText("This changed since you last read it.")).toBeNull();
    await user.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    await user.click(within(row).getByRole("button", { name: /cloud models/ }));
    const cloud = await screen.findByRole("dialog", {
      name: "Before PageLamp uses cloud models through Ollama",
    });
    expect(within(cloud).getByText("This changed since you last read it.")).toBeInTheDocument();
  });

  it("offers to replace a key missing from the keychain, not to add a first one", async () => {
    const mock = createMockApi({ latencyMs: 0, syncStepMs: 0, scenario: "ai-key" });
    const status = mock.aiStatus;
    vi.spyOn(mock, "aiStatus").mockImplementation(async () => {
      const s = await status();
      return {
        ...s,
        providers: s.providers.map((p) => ({ ...p, key_last4: null })),
        backends: s.backends.map((b) => ({ ...b, problems: ["key_missing" as const] })),
      };
    });
    const { user } = renderRoute("/settings", { api: mock });
    const section = await aiSection();
    const row = (await within(section).findByRole("heading", { name: "OpenAI" })).closest("li");
    if (!row) throw new Error("no OpenAI row");
    await user.click(await within(row).findByRole("button", { name: "Replace key" }));
    expect(within(row).queryByRole("button", { name: "Add a key" })).toBeNull();
    expect(
      await screen.findByRole("dialog", { name: "Replace the key for OpenAI" }),
    ).toBeInTheDocument();
  });

  it("says why a provider stays when the keychain keeps its key", async () => {
    const { user } = renderRoute("/settings", { scenario: "ai-keychain-locked" });
    const section = await aiSection();
    const row = (await within(section).findByRole("heading", { name: "OpenAI" })).closest("li");
    if (!row) throw new Error("no OpenAI row");
    await user.click(within(row).getByRole("button", { name: "Remove" }));
    const dialog = await screen.findByRole("alertdialog", { name: "Remove OpenAI?" });
    await user.click(within(dialog).getByRole("button", { name: "Remove" }));
    expect(await within(dialog).findByRole("alert")).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Technical details for OpenAI" }));
    expect(within(dialog).getByText(/Nothing was removed/)).toBeInTheDocument();
    expect(screen.queryByText("OpenAI removed")).toBeNull();
  });

  it("chooses and tests a model per feature", async () => {
    const { user } = renderRoute("/settings", { scenario: "ai-key" });
    const section = await aiSection();
    const plans = await within(section).findByRole("combobox", { name: "Model for Study plans" });
    expect(plans).toHaveTextContent("gpt-6-luna");
    const row = plans.closest("li");
    if (!row) throw new Error("no study plan row");
    await user.click(within(row).getByRole("button", { name: "Test" }));
    expect(await within(row).findByText("Works, answered in 0.9 s")).toBeInTheDocument();

    await user.click(plans);
    await user.click(await screen.findByRole("option", { name: /gpt-6-preview-0929/ }));
    expect(await within(row).findByText(/has no price for gpt-6-preview-0929/)).toBeInTheDocument();
  });

  it("shows the budget, warns near it, and can remove it", async () => {
    const { user } = renderRoute("/settings", { scenario: "ai-budget" });
    const section = await aiSection();
    expect(await within(section).findByText("$4.96 of $5.00 used this month")).toBeInTheDocument();
    expect(
      within(section).getByText("You've used 99% of this month's budget."),
    ).toBeInTheDocument();
    await user.click(within(section).getByLabelText("No budget"));
    await user.click(within(section).getByRole("button", { name: "Save budget" }));
    expect(await within(section).findByText("$4.96 used this month")).toBeInTheDocument();
  });

  it("explains model errors by code", async () => {
    const { user } = renderRoute("/settings", { scenario: "ai-errors" });
    const section = await aiSection();
    expect(await within(section).findByText(/Couldn't load the models from/)).toBeInTheDocument();
    const row = (
      await within(section).findByRole("combobox", { name: "Model for Study plans" })
    ).closest("li");
    if (!row) throw new Error("no row");
    await user.click(within(row).getByRole("button", { name: "Test" }));
    expect(
      await within(row).findByText("The provider is limiting requests. Try again in a moment."),
    ).toBeInTheDocument();
  });

  it("removes all AI data after asking", async () => {
    const { user } = renderRoute("/settings", { scenario: "ai-key" });
    const section = await aiSection();
    await within(section).findByText("Key ending in 7Qx2");
    await user.click(within(section).getByRole("button", { name: "Remove all AI data" }));
    const confirm = await screen.findByRole("alertdialog", { name: "Remove all AI data?" });
    expect(within(confirm).getByText(/answers about sharing materials stay/)).toBeInTheDocument();
    await user.click(within(confirm).getByRole("button", { name: "Remove all" }));
    expect(await within(section).findByText(/No model set up yet/)).toBeInTheDocument();
  });
});
