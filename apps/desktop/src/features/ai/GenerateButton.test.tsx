import { defaultScheduler, notifyManager } from "@tanstack/react-query";
import { act, screen, within } from "@testing-library/react";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { type AiBackendStatus, backendKey, type EstimateRequest } from "@/api/ai";
import { ApiError } from "@/api/errors";
import { createMockApi } from "@/api/mock";
import { DEMO101, DEMO205 } from "@/features/course/testing";
import { renderWithProviders } from "@/test/render";
import { GenerateButton } from "./GenerateButton";
import { MaterialSharingReminder, SharingNotAllowedNotice } from "./MaterialSharingNotices";

const explain = (course: string): EstimateRequest => ({ feature: "weekly_explanation", course });

function generate() {
  return screen.getByRole("button", { name: "Generate" });
}

describe("≈ $x before Generate", () => {
  it("shows the upper bound, rounded up to the cent, and generates", async () => {
    const onGenerate = vi.fn();
    const { user } = renderWithProviders(
      <GenerateButton request={explain(DEMO101)} onGenerate={onGenerate} />,
      { scenario: "ai-key" },
    );
    expect(await screen.findByText("≈ $0.07 at most")).toBeInTheDocument();
    expect(generate()).toHaveAccessibleDescription("Estimated cost: ≈ $0.07 at most");
    expect(screen.getByText("Up to 45K tokens in and 8K out")).toBeInTheDocument();
    await user.click(generate());
    expect(onGenerate).toHaveBeenCalledWith({ overrideBudget: false });
  });

  it("goes over the budget only when the student says so, for this run", async () => {
    const onGenerate = vi.fn();
    const { user } = renderWithProviders(
      <GenerateButton request={explain(DEMO101)} onGenerate={onGenerate} />,
      { scenario: "ai-budget" },
    );
    expect(await screen.findByText("This would go over this month's budget.")).toBeInTheDocument();
    expect(generate()).toHaveAttribute("aria-disabled", "true");
    await user.click(generate());
    expect(onGenerate).not.toHaveBeenCalled();
    await user.click(screen.getByLabelText("Go over the budget this time"));
    await user.click(generate());
    expect(onGenerate).toHaveBeenCalledWith({ overrideBudget: true });
  });

  it("keeps the tick through a refetch of the same request with the same block", async () => {
    const onGenerate = vi.fn();
    const { user, api, queryClient } = renderWithProviders(
      <GenerateButton request={explain(DEMO101)} onGenerate={onGenerate} />,
      { scenario: "ai-budget" },
    );
    const estimate = vi.spyOn(api, "estimateGeneration");
    expect(await screen.findByText("This would go over this month's budget.")).toBeInTheDocument();
    await user.click(screen.getByLabelText("Go over the budget this time"));
    expect(generate()).not.toHaveAttribute("aria-disabled");
    await act(async () => {
      await queryClient.invalidateQueries();
    });
    expect(estimate).toHaveBeenCalled();
    expect(screen.getByLabelText("Go over the budget this time")).toBeChecked();
    expect(generate()).not.toHaveAttribute("aria-disabled");
    await user.click(generate());
    expect(onGenerate).toHaveBeenCalledWith({ overrideBudget: true });
  });

  it("waits for a changed request's own estimate before it runs", async () => {
    const onGenerate = vi.fn();
    function Switching() {
      const [include, setInclude] = useState<string[]>([]);
      return (
        <>
          <button type="button" onClick={() => setInclude(["no-such-material"])}>
            Change
          </button>
          <GenerateButton
            request={{ feature: "weekly_explanation", course: DEMO101, include }}
            onGenerate={onGenerate}
          />
        </>
      );
    }
    const { user } = renderWithProviders(<Switching />, { scenario: "ai-key" });
    expect(await screen.findByText("≈ $0.07 at most")).toBeInTheDocument();
    await vi.waitFor(() => expect(generate()).not.toHaveAttribute("aria-disabled"));
    await user.click(screen.getByRole("button", { name: "Change" }));
    // The previous request's "≈ $x" is still shown, but nothing runs from it.
    expect(generate()).toHaveAttribute("aria-disabled", "true");
    await user.click(generate());
    expect(onGenerate).not.toHaveBeenCalled();
    await vi.waitFor(() => expect(generate()).not.toHaveAttribute("aria-disabled"));
  });

  it("asks once before using a model with no price", async () => {
    const { user, api } = renderWithProviders(
      <GenerateButton request={explain(DEMO101)} onGenerate={() => {}} />,
      { scenario: "ai-unpriced" },
    );
    const acknowledge = vi.spyOn(api, "acknowledgeUnpricedModel");
    expect(await screen.findByText("No price for this model")).toBeInTheDocument();
    expect(generate()).toHaveAttribute("aria-disabled", "true");
    await user.click(screen.getByRole("button", { name: "Use it anyway" }));
    expect(acknowledge).toHaveBeenCalledWith(
      { kind: "provider", provider_id: "openai" },
      "gpt-6-preview-0929",
    );
    await vi.waitFor(() => expect(generate()).not.toHaveAttribute("aria-disabled"));
  });

  it("moves focus on only once the estimate asked again is shown", async () => {
    // The page hears of new query data a while after the call that asked for it returned.
    notifyManager.setScheduler((cb) => setTimeout(cb, 40));
    try {
      const { user } = renderWithProviders(
        <GenerateButton request={explain(DEMO101)} onGenerate={() => {}} />,
        { scenario: "ai-unpriced" },
      );
      await user.click(await screen.findByRole("button", { name: "Use it anyway" }));
      await vi.waitFor(() => expect(generate()).not.toHaveAttribute("aria-disabled"));
      await vi.waitFor(() => expect(generate()).toHaveFocus());
    } finally {
      notifyManager.setScheduler(defaultScheduler);
    }
  });

  it("is free on this computer", async () => {
    renderWithProviders(<GenerateButton request={explain(DEMO205)} onGenerate={() => {}} />, {
      scenario: "ai-local",
    });
    expect(await screen.findByText("Free, on this computer")).toBeInTheDocument();
  });

  it("explains why it can't run, and where to fix it", async () => {
    renderWithProviders(
      <GenerateButton request={{ feature: "weekly_note" }} onGenerate={() => {}} />,
    );
    expect(await screen.findByText(/Choose a model for this/)).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "AI models" })).toHaveAttribute("href", "/settings");
    expect(generate()).toHaveAttribute("aria-disabled", "true");
  });

  it("says there's nothing to write about, with no cost, no tokens and no Settings link", async () => {
    const api = createMockApi({ latencyMs: 0, syncStepMs: 0, scenario: "ai-key" });
    vi.spyOn(api, "estimateGeneration").mockResolvedValue({
      micro_usd_upper: null,
      input_tokens: 0,
      max_output_tokens: 0,
      reasoning_allowance: 0,
      repair_possible: false,
      price_known: false,
      would_block: "nothing_to_write",
    });
    const onGenerate = vi.fn();
    const { user } = renderWithProviders(
      <GenerateButton request={{ feature: "weekly_note" }} onGenerate={onGenerate} />,
      { api },
    );
    expect(await screen.findByText(/nothing to write about this week yet/)).toBeInTheDocument();
    expect(generate()).toHaveAttribute("aria-disabled", "true");
    expect(screen.queryByRole("link", { name: "AI models" })).toBeNull();
    // No cost line at all: not even "No price for this model", which the blocked estimate's empty
    // price would read as if the cost line showed for it.
    expect(screen.queryByText(/No price for this model/)).toBeNull();
    await user.click(generate());
    expect(onGenerate).not.toHaveBeenCalled();
  });

  it("never sends a course whose materials may not be shared to a cloud model", async () => {
    renderWithProviders(<GenerateButton request={explain(DEMO205)} onGenerate={() => {}} />, {
      scenario: "ai-key",
    });
    expect(
      await screen.findByText(/may not be shared with AI services, so nothing was sent/),
    ).toBeInTheDocument();
    expect(generate()).toHaveAttribute("aria-disabled", "true");
  });
});

describe("question (b) notices", () => {
  it("reminds once, without blocking, and records the answer given inline", async () => {
    const onClose = vi.fn();
    const { user, api } = renderWithProviders(
      <MaterialSharingReminder
        courseId={DEMO101}
        courseName="DEMO101"
        service="OpenAI"
        onClose={onClose}
      />,
    );
    const save = vi.spyOn(api, "setCourseMaterialSharing");
    expect(
      screen.getByRole("heading", {
        name: "Check whether your instructor allows sharing course materials with AI services",
      }),
    ).toBeInTheDocument();
    expect(screen.getByText(/sent text from DEMO101's materials to OpenAI/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Yes, it's allowed" }));
    expect(save).toHaveBeenCalledWith(DEMO101, "allowed");
    await vi.waitFor(() => expect(onClose).toHaveBeenCalled());
  });

  it("offers what still works when sharing isn't allowed", async () => {
    const onUseLocal = vi.fn();
    const { user } = renderWithProviders(
      <SharingNotAllowedNotice courseId={DEMO205} courseName="DEMO205" onUseLocal={onUseLocal} />,
    );
    expect(screen.getByText(/DEMO205's materials may not be shared/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /structure only/ })).toBeNull();
    await user.click(screen.getByRole("button", { name: "Use a model on this computer" }));
    expect(onUseLocal).toHaveBeenCalled();
    expect(screen.getByRole("link", { name: "Change the answer" })).toHaveAttribute(
      "href",
      `/courses/${encodeURIComponent(DEMO205)}?tab=policy`,
    );
  });
});

describe("a model that a server on this computer may run in the cloud", () => {
  const OLLAMA = { kind: "provider", provider_id: "ollama" } as const;
  const CLOUD = { backend: OLLAMA, model: "gpt-oss:120b-cloud", effort: "lowest" } as const;

  /** Ollama set up and its own note read, with the explanations routed to its cloud model. */
  async function cloudViaLocal() {
    const api = createMockApi({ latencyMs: 0, syncStepMs: 0, scenario: "ai-local" });
    await api.setFeatureModel("weekly_explanation", CLOUD);
    return api;
  }

  it("asks for its own note, then its unknown price, and runs nothing before both", async () => {
    const api = await cloudViaLocal();
    const onGenerate = vi.fn();
    const runsHere = vi.spyOn(api, "modelRunsHere");
    const note = vi.spyOn(api, "modelDisclosure");
    const read = vi.spyOn(api, "acknowledgeModelDisclosure");
    const { user } = renderWithProviders(
      <GenerateButton request={explain(DEMO101)} onGenerate={onGenerate} />,
      { api },
    );
    // The server's own note ("nothing leaves this computer") doesn't cover this model.
    expect(
      await screen.findByText("Read what this service receives, then turn it on."),
    ).toBeInTheDocument();
    expect(generate()).toHaveAttribute("aria-disabled", "true");
    await user.click(generate());
    expect(onGenerate).not.toHaveBeenCalled();

    expect(screen.getByRole("button", { name: "Read it now" })).toHaveAccessibleDescription(
      "Read what this service receives, then turn it on.",
    );
    await user.click(screen.getByRole("button", { name: "Read it now" }));
    const sheet = await screen.findByRole("dialog", { name: "Before PageLamp uses Ollama" });
    // Only their materials are withheld: what it promises, no more.
    expect(
      within(sheet).getByText(
        /The materials of courses you answered “No, it's not allowed” for are never sent to it\.$/,
      ),
    ).toBeInTheDocument();
    // Never read before: nothing changed (the server's own note was read, in its own version).
    expect(within(sheet).queryByText("This changed since you last read it.")).toBeNull();
    // Where it runs is decided as the run decides it: the server is asked.
    expect(runsHere).toHaveBeenCalledWith(CLOUD, true);
    expect(note).toHaveBeenCalledWith(OLLAMA, false);
    expect(
      within(sheet).getByText(
        /^Ollama runs on this computer, but PageLamp can't confirm that this model does/,
      ),
    ).toBeInTheDocument();
    expect(
      within(sheet).getByText(/^This model may run in the cloud through Ollama/),
    ).toBeInTheDocument();
    await user.click(within(sheet).getByRole("button", { name: "Use gpt-oss:120b-cloud" }));
    expect(read).toHaveBeenCalledWith(OLLAMA, false, expect.any(Number));

    // Then its unknown price, still before anything runs; focus is on it.
    const unpriced = await screen.findByRole("button", { name: "Use it anyway" });
    await vi.waitFor(() => expect(unpriced).toHaveFocus());
    expect(unpriced).toHaveAccessibleDescription(
      "Your budget isn't enforced for this model. Its runs are still counted in tokens.",
    );
    expect(screen.getByText("May run in the cloud; no cost estimate")).toBeInTheDocument();
    expect(generate()).toHaveAttribute("aria-disabled", "true");
    await user.click(generate());
    expect(onGenerate).not.toHaveBeenCalled();
    await user.click(unpriced);
    await vi.waitFor(() => expect(generate()).not.toHaveAttribute("aria-disabled"));
    // Focus moves on to Generate, not to the page.
    await vi.waitFor(() => expect(generate()).toHaveFocus());
    expect(onGenerate).not.toHaveBeenCalled();
    await user.click(generate());
    expect(onGenerate).toHaveBeenCalledTimes(1);
  });

  it("says so, and shows and runs nothing, when the server can't be asked", async () => {
    const api = await cloudViaLocal();
    vi.spyOn(api, "modelRunsHere").mockRejectedValue(
      new ApiError("model", "Couldn't reach Ollama.", { model_error: "network" }),
    );
    const note = vi.spyOn(api, "modelDisclosure");
    const { user } = renderWithProviders(
      <GenerateButton request={explain(DEMO101)} onGenerate={() => {}} />,
      { api },
    );
    await user.click(await screen.findByRole("button", { name: "Read it now" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(/\S/);
    expect(note).not.toHaveBeenCalled();
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(generate()).toHaveAttribute("aria-disabled", "true");
  });

  it("shows a service's own note the same way when that is the one to read", async () => {
    const { user, api } = renderWithProviders(
      <GenerateButton request={explain(DEMO101)} onGenerate={() => {}} />,
      { scenario: "ai-disclosure-changed" },
    );
    const read = vi.spyOn(api, "acknowledgeModelDisclosure");
    await user.click(await screen.findByRole("button", { name: "Read it now" }));
    const sheet = await screen.findByRole("dialog", { name: "Before PageLamp uses Anthropic" });
    // Read again because it changed, and with its age confirmation first, as in Settings.
    expect(within(sheet).getByText("This changed since you last read it.")).toBeInTheDocument();
    const accept = within(sheet).getByRole("button", { name: "Turn on Anthropic" });
    expect(accept).toHaveAttribute("aria-disabled", "true");
    const boxes = within(sheet).getAllByRole("checkbox");
    expect(boxes).toHaveLength(1);
    for (const box of boxes) await user.click(box);
    await user.click(accept);
    expect(read).toHaveBeenCalledWith(
      { kind: "provider", provider_id: "anthropic" },
      false,
      expect.any(Number),
    );
    await vi.waitFor(() => expect(generate()).not.toHaveAttribute("aria-disabled"));
  });

  it("shows the server's own note for a model it runs here, and goes on to Generate", async () => {
    const api = createMockApi({ latencyMs: 0, syncStepMs: 0 });
    await api.addModelProvider("ollama", "http://127.0.0.1:11434", null);
    const local = { backend: OLLAMA, model: "qwen3.5:9b", effort: "lowest" } as const;
    await api.setFeatureModel("weekly_explanation", local);
    const note = vi.spyOn(api, "modelDisclosure");
    const read = vi.spyOn(api, "acknowledgeModelDisclosure");
    const { user } = renderWithProviders(
      <GenerateButton request={explain(DEMO205)} onGenerate={() => {}} />,
      { api },
    );
    await user.click(await screen.findByRole("button", { name: "Read it now" }));
    const sheet = await screen.findByRole("dialog", { name: "Before PageLamp uses Ollama" });
    expect(note).toHaveBeenCalledWith(OLLAMA, true);
    expect(
      within(sheet).getByText("It goes to Ollama on this computer and doesn't leave it."),
    ).toBeInTheDocument();
    await user.click(within(sheet).getByRole("button", { name: "Turn on Ollama" }));
    expect(read).toHaveBeenCalledWith(OLLAMA, true, expect.any(Number));
    // Free here, so no price to acknowledge: Generate is next, with focus.
    await vi.waitFor(() => expect(generate()).not.toHaveAttribute("aria-disabled"));
    await vi.waitFor(() => expect(generate()).toHaveFocus());
    expect(screen.queryByRole("button", { name: "Use it anyway" })).toBeNull();
  });

  it("goes on to the next step when the note turns out read already", async () => {
    const api = await cloudViaLocal();
    const { user } = renderWithProviders(
      <GenerateButton request={explain(DEMO101)} onGenerate={() => {}} />,
      { api },
    );
    const button = await screen.findByRole("button", { name: "Read it now" });
    // Read meanwhile (e.g. in Settings, in another window).
    const [facts] = await api.modelDisclosure(OLLAMA, false);
    await api.acknowledgeModelDisclosure(OLLAMA, false, facts.version);
    await user.click(button);
    const unpriced = await screen.findByRole("button", { name: "Use it anyway" });
    await vi.waitFor(() => expect(unpriced).toHaveFocus());
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("sends to Settings when the note is read but the estimate still asks for one", async () => {
    const api = await cloudViaLocal();
    const disclosure = api.modelDisclosure;
    vi.spyOn(api, "modelDisclosure").mockImplementation(async (backend, here) => {
      const [facts] = await disclosure(backend, here);
      return [facts, true];
    });
    const estimate = vi.spyOn(api, "estimateGeneration");
    const { user } = renderWithProviders(
      <GenerateButton request={explain(DEMO101)} onGenerate={() => {}} />,
      { api },
    );
    const button = await screen.findByRole("button", { name: "Read it now" });
    const before = estimate.mock.calls.length;
    await user.click(button);
    // Asked again, and still blocked: Settings has every note of the backend.
    const settings = await screen.findByRole("link", { name: "AI models" });
    expect(estimate.mock.calls.length).toBeGreaterThan(before);
    expect(settings).toHaveAttribute("href", "/settings");
    expect(settings).toHaveAccessibleDescription(
      "Read what this service receives, then turn it on.",
    );
    await vi.waitFor(() => expect(settings).toHaveFocus());
    expect(screen.queryByRole("button", { name: "Read it now" })).toBeNull();
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("shows a ChatGPT plan's note in place once the plan is set up", async () => {
    const { user, api } = renderWithProviders(
      <GenerateButton request={{ feature: "weekly_note" }} onGenerate={() => {}} />,
      { scenario: "codex-edu" },
    );
    const note = vi.spyOn(api, "modelDisclosure");
    await user.click(await screen.findByRole("button", { name: "Read it now" }));
    expect(
      await screen.findByRole("dialog", { name: /^Before PageLamp uses / }),
    ).toBeInTheDocument();
    expect(note).toHaveBeenCalledWith({ kind: "codex" }, false);
    expect(screen.queryByRole("link", { name: "AI models" })).toBeNull();
  });

  /** ai_status with Ollama's accepted versions of its notes changed by `edit`. */
  function withAccepted(
    api: Awaited<ReturnType<typeof cloudViaLocal>>,
    edit: (b: AiBackendStatus) => Partial<AiBackendStatus>,
  ) {
    const status = api.aiStatus;
    vi.spyOn(api, "aiStatus").mockImplementation(async () => {
      const s = await status();
      return {
        ...s,
        backends: s.backends.map((b) =>
          backendKey(b.backend) === backendKey(OLLAMA)
            ? { ...b, ...edit(b), problems: [...b.problems, "disclosure_changed" as const] }
            : b,
        ),
      };
    });
  }

  it("says the note of its cloud models changed when that is the one read before", async () => {
    const api = await cloudViaLocal();
    withAccepted(api, (b) => ({
      cloud_disclosure_acknowledged: (b.cloud_disclosure?.version ?? 1) - 1,
    }));
    const { user } = renderWithProviders(
      <GenerateButton request={explain(DEMO101)} onGenerate={() => {}} />,
      { api },
    );
    await user.click(await screen.findByRole("button", { name: "Read it now" }));
    const sheet = await screen.findByRole("dialog", { name: "Before PageLamp uses Ollama" });
    expect(within(sheet).getByText(/can't confirm that this model does/)).toBeInTheDocument();
    expect(within(sheet).getByText("This changed since you last read it.")).toBeInTheDocument();
  });

  it("doesn't say a note never read changed because the other one did", async () => {
    const api = createMockApi({ latencyMs: 0, syncStepMs: 0 });
    await api.addModelProvider("ollama", "http://127.0.0.1:11434", null);
    await api.setFeatureModel("weekly_explanation", {
      backend: OLLAMA,
      model: "qwen3.5:9b",
      effort: "lowest",
    });
    // Its cloud models' note was read in an older version; its own never.
    withAccepted(api, (b) => ({
      cloud_disclosure_acknowledged: (b.cloud_disclosure?.version ?? 1) - 1,
    }));
    const { user } = renderWithProviders(
      <GenerateButton request={explain(DEMO205)} onGenerate={() => {}} />,
      { api },
    );
    await user.click(await screen.findByRole("button", { name: "Read it now" }));
    const sheet = await screen.findByRole("dialog", { name: "Before PageLamp uses Ollama" });
    expect(within(sheet).getByText(/doesn't leave it/)).toBeInTheDocument();
    expect(within(sheet).queryByText("This changed since you last read it.")).toBeNull();
  });

  it("still says why and where to go when the AI status can't be read", async () => {
    const api = await cloudViaLocal();
    vi.spyOn(api, "aiStatus").mockRejectedValue(new ApiError("internal", "Synthetic failure"));
    renderWithProviders(<GenerateButton request={explain(DEMO101)} onGenerate={() => {}} />, {
      api,
    });
    expect(
      await screen.findByText("Read what this service receives, then turn it on."),
    ).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "AI models" })).toHaveAttribute("href", "/settings");
    expect(generate()).toHaveAttribute("aria-disabled", "true");
  });

  it("leaves focus where the student put it while a step was settling", async () => {
    let release = () => {};
    const { user, api } = renderWithProviders(
      <>
        <GenerateButton request={explain(DEMO101)} onGenerate={() => {}} />
        <input aria-label="Elsewhere" />
      </>,
      { scenario: "ai-unpriced" },
    );
    const acknowledge = api.acknowledgeUnpricedModel;
    vi.spyOn(api, "acknowledgeUnpricedModel").mockImplementation(async (backend, model) => {
      await new Promise<void>((resolve) => {
        release = resolve;
      });
      return acknowledge(backend, model);
    });
    await user.click(await screen.findByRole("button", { name: "Use it anyway" }));
    const elsewhere = screen.getByRole("textbox", { name: "Elsewhere" });
    await user.click(elsewhere);
    release();
    await vi.waitFor(() => expect(generate()).not.toHaveAttribute("aria-disabled"));
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(elsewhere).toHaveFocus();
  });

  it("moves on to the next step when the sheet was closed before its accept was done", async () => {
    const api = await cloudViaLocal();
    let release = () => {};
    const read = api.acknowledgeModelDisclosure;
    vi.spyOn(api, "acknowledgeModelDisclosure").mockImplementation(async (...args) => {
      await new Promise<void>((resolve) => {
        release = resolve;
      });
      return read(...args);
    });
    const { user } = renderWithProviders(
      <GenerateButton request={explain(DEMO101)} onGenerate={() => {}} />,
      { api },
    );
    await user.click(await screen.findByRole("button", { name: "Read it now" }));
    const sheet = await screen.findByRole("dialog", { name: "Before PageLamp uses Ollama" });
    await user.click(within(sheet).getByRole("button", { name: "Use gpt-oss:120b-cloud" }));
    await user.keyboard("{Escape}");
    await vi.waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    release();
    const unpriced = await screen.findByRole("button", { name: "Use it anyway" });
    await vi.waitFor(() => expect(unpriced).toHaveFocus());
  });

  it("sends a ChatGPT plan that isn't set up to Settings, not to its note", async () => {
    renderWithProviders(
      <GenerateButton request={{ feature: "weekly_note" }} onGenerate={() => {}} />,
      {
        scenario: "codex-free",
      },
    );
    expect(
      await screen.findByText(/^Read what this service receives, then turn it on\./),
    ).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "AI models" })).toHaveAttribute("href", "/settings");
    expect(screen.queryByRole("button", { name: "Read it now" })).toBeNull();
  });
});
