import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import i18n from "@/i18n";
import { AiGeneratedLabel, aiGeneratedLabelText } from "./AiGeneratedLabel";

const META = {
  backend_label: "OpenAI",
  model: "gpt-6-luna",
  created_at: "2026-10-01T14:30:00Z",
  usage: { input_tokens: 41_000, output_tokens: 2_600 },
  estimated: false,
};

describe("AI-generated label", () => {
  it("names the backend, model, date and tokens", () => {
    render(<AiGeneratedLabel meta={META} />);
    expect(
      screen.getByText("AI-generated · OpenAI · gpt-6-luna · Oct 1, 2026 · 43,600 tokens"),
    ).toBeInTheDocument();
  });

  it("takes the course calendar's AiLabel as is, without tokens", () => {
    const label = {
      backend_label: "Claude",
      model: "claude-haiku-4-5",
      created_at: META.created_at,
    };
    expect(aiGeneratedLabelText(label, i18n.getFixedT("en", "ai"), "en")).toBe(
      "AI-generated · Claude · claude-haiku-4-5 · Oct 1, 2026",
    );
  });

  it("marks estimated tokens, and reads naturally in Chinese", () => {
    const text = aiGeneratedLabelText(
      { ...META, estimated: true },
      i18n.getFixedT("zh-CN", "ai"),
      "zh-CN",
    );
    expect(text).toBe("AI 生成 · OpenAI · gpt-6-luna · 2026年10月1日 · ≈ 43,600 个 token");
  });
});
