import { screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
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

  it("explains model errors by code, with the provider's retry delay", async () => {
    const { user } = renderRoute("/settings", { scenario: "ai-errors" });
    const section = await aiSection();
    expect(await within(section).findByText(/Couldn't load the models from/)).toBeInTheDocument();
    const row = (
      await within(section).findByRole("combobox", { name: "Model for Study plans" })
    ).closest("li");
    if (!row) throw new Error("no row");
    await user.click(within(row).getByRole("button", { name: "Test" }));
    expect(await within(row).findByText(/Try again in 20 s/)).toBeInTheDocument();
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
