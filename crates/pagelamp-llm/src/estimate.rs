//! The pre-flight estimate: an UPPER bound on what one generation can cost, shown as "≈ $x"
//! before Generate and checked against the monthly budget (design §3.5). Deliberately not
//! "characters ÷ 4", which runs low for Chinese text and code.

use pagelamp_core::ai::Effort;
use pagelamp_core::ai_gate::RenderedPrompt;

use crate::catalog::{self, cost_micro_usd};
use crate::profile::{ProviderProfile, Wire};
use crate::request::{OutputSpec, Usage};

/// Tokens added per request for message framing, the answer-format instructions a provider
/// adds for JSON, and the repair instruction [estimate; tune against recorded usage].
const REQUEST_OVERHEAD_TOKENS: u64 = 300;

/// An upper bound on one generation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Estimate {
    pub input_tokens: u64,
    pub max_output_tokens: u64,
    /// Output tokens a model may spend thinking beyond `max_output_tokens`.
    pub reasoning_allowance: u64,
    /// A repair call may follow (JSON below the native tier): everything counted twice.
    pub repair_possible: bool,
    /// `None` when the model's price is unknown (the budget can't be enforced for it).
    pub micro_usd_upper: Option<u64>,
    pub price_known: bool,
}

/// Tokens per CJK character, in tenths, by the tokenizer family behind a wire
/// [estimate; tune against recorded usage]: OpenAI's o200k needs about one per common
/// character; for the others (Anthropic, Gemini, open models behind compatible servers) PageLamp
/// has no measurement yet, so it assumes half as much again.
pub fn cjk_tenths(wire: Wire) -> u64 {
    match wire {
        Wire::OpenAiResponses => 10,
        Wire::AnthropicMessages | Wire::OpenAiChat | Wire::OllamaNative => 15,
    }
}

/// A conservative token count of `text`: `cjk_tenths / 10` per CJK character (one more outside
/// the Basic Multilingual Plane), one per 3 UTF-8 bytes of everything else.
pub fn count_tokens_upper(text: &str, cjk_tenths: u64) -> u64 {
    let (mut cjk, mut rare, mut other_bytes) = (0u64, 0u64, 0u64);
    for ch in text.chars() {
        if is_cjk(ch) {
            cjk += 1;
            rare += u64::from(u32::from(ch) > 0xFFFF);
        } else {
            other_bytes += ch.len_utf8() as u64;
        }
    }
    (cjk * cjk_tenths).div_ceil(10) + rare + other_bytes.div_ceil(3)
}

fn is_cjk(ch: char) -> bool {
    matches!(u32::from(ch),
        0x2E80..=0x2FDF      // CJK radicals
        | 0x3000..=0x303F    // CJK symbols and punctuation
        | 0x3040..=0x30FF    // Hiragana, Katakana
        | 0x3100..=0x31FF    // Bopomofo, Hangul compatibility, Kanbun
        | 0x3400..=0x4DBF    // CJK extension A
        | 0x4E00..=0x9FFF    // CJK unified ideographs
        | 0xAC00..=0xD7AF    // Hangul syllables
        | 0xF900..=0xFAFF    // CJK compatibility ideographs
        | 0xFF00..=0xFFEF    // full-width forms
        | 0x20000..=0x3134F  // CJK extensions B–G
    )
}

/// The upper bound for sending `prompt` to `model` of `profile`.
pub fn estimate(
    profile: &ProviderProfile,
    model: &str,
    prompt: &RenderedPrompt,
    output: &OutputSpec,
    effort: Effort,
    max_output_tokens: u32,
) -> Estimate {
    let quirks = profile.model_quirks(model);
    let entry = catalog::lookup(&profile.id, model);
    let cjk = cjk_tenths(profile.wire);
    let schema_tokens = match output {
        OutputSpec::Json { schema, .. } => count_tokens_upper(&schema.to_string(), cjk),
        OutputSpec::Text => 0,
    };
    let input_tokens = count_tokens_upper(prompt.instructions(), cjk)
        + count_tokens_upper(prompt.user_text(), cjk)
        + schema_tokens
        + REQUEST_OVERHEAD_TOKENS;
    let max_output = u64::from(max_output_tokens);
    // Responses, Messages and Ollama count thinking inside the output cap; a generic
    // compatible server may not.
    let cap_includes_thinking = !matches!(profile.wire, Wire::OpenAiChat);
    let thinks = quirks.thinking_always_on || effort != Effort::Lowest;
    let reasoning_allowance = if thinks && !cap_includes_thinking {
        max_output
    } else {
        0
    };
    let repair_possible = matches!(output, OutputSpec::Json { .. })
        && (profile.quirks.json_object_only
            || profile.quirks.no_json_mode
            || entry.is_some_and(|model| !model.structured_output));
    let (micro_usd_upper, price_known) = if profile.on_device() {
        (Some(0), true)
    } else if let Some(entry) = entry {
        let first = Usage {
            input_uncached: input_tokens,
            output: max_output + reasoning_allowance,
            ..Usage::default()
        };
        let mut cost = cost_micro_usd(entry.prices(input_tokens), &first);
        if repair_possible {
            // The repair re-sends the prompt plus the first answer.
            let repair_input = input_tokens + max_output;
            let repair = Usage {
                input_uncached: repair_input,
                output: max_output + reasoning_allowance,
                ..Usage::default()
            };
            cost = cost.saturating_add(cost_micro_usd(entry.prices(repair_input), &repair));
        }
        (Some(cost), true)
    } else {
        (None, false)
    };
    Estimate {
        input_tokens,
        max_output_tokens: max_output,
        reasoning_allowance,
        repair_possible,
        micro_usd_upper,
        price_known,
    }
}

/// What `usage` of `model` cost (the ledger), if its price is known; 0 on this computer.
pub fn usage_cost(profile: &ProviderProfile, model: &str, usage: &Usage) -> Option<u64> {
    if profile.on_device() {
        return Some(0);
    }
    let entry = catalog::lookup(&profile.id, model)?;
    let input = usage.input_uncached + usage.cache_read + usage.cache_write;
    Some(cost_micro_usd(entry.prices(input), usage))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chinese_counts_per_character_and_ascii_one_per_three_bytes() {
        assert_eq!(count_tokens_upper("", 10), 0);
        assert_eq!(count_tokens_upper("abc", 10), 1);
        assert_eq!(count_tokens_upper("abcd", 10), 2);
        assert_eq!(count_tokens_upper("光合作用", 10), 4);
        assert_eq!(count_tokens_upper("光合作用", 15), 6);
        assert_eq!(count_tokens_upper("光合作用 is photosynthesis", 10), 4 + 6);
        assert_eq!(count_tokens_upper("𠀀", 10), 2, "outside the BMP");
        assert_eq!(count_tokens_upper("é", 10), 1);
    }
}
