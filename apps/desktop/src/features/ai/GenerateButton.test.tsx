import { screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { EstimateRequest } from "@/api/ai";
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

    await user.click(screen.getByRole("button", { name: "Read it now" }));
    const sheet = await screen.findByRole("dialog", { name: "Before PageLamp uses Ollama" });
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
    expect(generate()).toHaveAttribute("aria-disabled", "true");
    await user.click(generate());
    expect(onGenerate).not.toHaveBeenCalled();
    await user.click(unpriced);
    await vi.waitFor(() => expect(generate()).not.toHaveAttribute("aria-disabled"));
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

  it("asks for the estimate again when the note turns out read already", async () => {
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
    await vi.waitFor(() => expect(estimate.mock.calls.length).toBeGreaterThan(before));
    expect(screen.queryByRole("dialog")).toBeNull();
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
