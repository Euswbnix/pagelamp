import { screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { aiKeys } from "@/api/ai-queries";
import { createMockApi } from "@/api/mock";
import type { MockScenario } from "@/api/mock/fixtures";
import { renderRoute } from "@/test/render";

async function openDialog(scenario: MockScenario) {
  const api = createMockApi({ latencyMs: 0, scenario });
  const { user, queryClient } = renderRoute("/settings", { api });
  const button = await screen.findByRole("button", { name: "Remove all AI data" });
  // The dialog's words follow the ChatGPT sign-in: wait until it has been read.
  await waitFor(() => expect(queryClient.getQueryData(aiKeys.codex())).toBeDefined());
  await user.click(button);
  const dialog = await screen.findByRole("alertdialog", { name: "Remove all AI data?" });
  return { dialog, user };
}

describe("Remove all AI data", () => {
  it("names the ChatGPT sign-out when there is one to undo", async () => {
    const { dialog } = await openDialog("codex-plus");
    expect(within(dialog).getByText(/signs PageLamp's Codex out of ChatGPT/)).toBeInTheDocument();
    expect(within(dialog).getByText(/what PageLamp's AI features wrote/)).toBeInTheDocument();
    expect(
      within(dialog).getByText(/calendar you accepted .* stays, with its AI label/),
    ).toBeInTheDocument();
  });

  it.each(["ai-key", "codex-signed-out"] as const)(
    "doesn't mention ChatGPT without a ChatGPT sign-in (%s)",
    async (scenario) => {
      const { dialog } = await openDialog(scenario);
      expect(within(dialog).getByText(/what PageLamp's AI features wrote\./)).toBeInTheDocument();
      expect(within(dialog).getByText(/stays, with its AI label/)).toBeInTheDocument();
      expect(within(dialog).queryByText(/ChatGPT|Codex/)).toBeNull();
      expect(within(dialog).queryByText(/every plan and explanation/)).toBeNull();
    },
  );

  it("never says all was removed when the keychain keeps a key, and shows what stayed", async () => {
    const { dialog, user } = await openDialog("ai-keychain-locked");
    await user.click(within(dialog).getByRole("button", { name: "Remove all" }));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent(
      "Something unexpected went wrong.",
    );
    await user.click(
      within(dialog).getByRole("button", { name: "Technical details for Remove all AI data" }),
    );
    expect(
      within(dialog).getByText(/delete the API key of openai: it is still there \(`llm:openai`\)/),
    ).toBeInTheDocument();
    expect(screen.queryByText("All AI data removed")).toBeNull();
    // Behind it, Settings shows what is left: the provider whose key stayed, its note to read.
    await user.click(within(dialog).getByRole("button", { name: "Cancel" }));
    const row = (await screen.findByRole("heading", { name: "OpenAI" })).closest("li");
    if (!row) throw new Error("no OpenAI row");
    await waitFor(() => expect(within(row).getByText("Not turned on yet")).toBeInTheDocument());
    expect(screen.queryByRole("heading", { name: "Anthropic" })).toBeNull();
    // A new try starts without the earlier error.
    await user.click(screen.getByRole("button", { name: "Remove all AI data" }));
    const again = await screen.findByRole("alertdialog", { name: "Remove all AI data?" });
    expect(within(again).queryByRole("alert")).toBeNull();
  });

  it("keeps the dialog and its error when Settings can't be read again after it", async () => {
    const api = createMockApi({ latencyMs: 0, scenario: "ai-keychain-locked" });
    const { user, queryClient } = renderRoute("/settings", { api });
    const button = await screen.findByRole("button", { name: "Remove all AI data" });
    await waitFor(() => expect(queryClient.getQueryData(aiKeys.codex())).toBeDefined());
    await user.click(button);
    const dialog = await screen.findByRole("alertdialog", { name: "Remove all AI data?" });
    // The key that stayed can't be read either: reading the AI settings again fails.
    const remove = api.removeAllAiData;
    vi.spyOn(api, "removeAllAiData").mockImplementation(async () => {
      vi.spyOn(api, "aiStatus").mockRejectedValue(new Error("Synthetic failure"));
      return remove();
    });
    await user.click(within(dialog).getByRole("button", { name: "Remove all" }));
    expect(await within(dialog).findByRole("alert")).toBeInTheDocument();
    expect(await screen.findByText(/^Couldn't load the AI settings\./)).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: /^Technical details/ })).toBeInTheDocument();
    expect(screen.getByRole("alertdialog", { name: "Remove all AI data?" })).toBe(dialog);
  });
});
