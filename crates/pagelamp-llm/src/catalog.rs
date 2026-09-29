//! The vendored price and capability snapshot (`data/prices.json`, from models.dev, MIT; see
//! `data/models-dev.LICENSE`). Refreshed per release by `scripts/refresh-prices.mjs`; never
//! fetched at runtime (owner decision D20). Money is integer micro-USD throughout.

use std::collections::HashMap;
use std::sync::LazyLock;

use serde::Deserialize;

use crate::request::Usage;

/// Prices of one model for one prompt size, in micro-USD per million tokens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Prices {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
struct PriceTier {
    above: u64,
    input: u64,
    output: u64,
    cache_read: Option<u64>,
    cache_write: Option<u64>,
}

/// One model of the snapshot.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct CatalogModel {
    input: u64,
    output: u64,
    cache_read: Option<u64>,
    cache_write: Option<u64>,
    #[serde(default)]
    tiers: Vec<PriceTier>,
    /// Context window in tokens.
    pub context: Option<u64>,
    pub max_output: Option<u64>,
    /// The provider enforces JSON schemas for this model.
    pub structured_output: bool,
    /// The model takes an effort setting.
    pub effort: bool,
    /// The cheapest effort value it takes (`none`, `minimal`, `low`).
    pub lowest_effort: Option<String>,
    pub thinking_always_on: bool,
}

impl CatalogModel {
    /// The prices for a prompt of `input_tokens` (the context tier it falls in).
    pub fn prices(&self, input_tokens: u64) -> Prices {
        let tier = self
            .tiers
            .iter()
            .rev()
            .find(|tier| input_tokens > tier.above);
        let (input, output, cache_read, cache_write) = match tier {
            Some(tier) => (tier.input, tier.output, tier.cache_read, tier.cache_write),
            None => (self.input, self.output, self.cache_read, self.cache_write),
        };
        Prices {
            input,
            output,
            // Unlisted cache prices: charged like input (an upper bound for reads).
            cache_read: cache_read.unwrap_or(input),
            cache_write: cache_write.unwrap_or(input),
        }
    }
}

#[derive(Deserialize)]
struct Snapshot {
    fetched: String,
    providers: HashMap<String, HashMap<String, CatalogModel>>,
}

static SNAPSHOT: LazyLock<Snapshot> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../data/prices.json"))
        .expect("data/prices.json is valid (tested)")
});

/// The day the snapshot was taken (shown next to estimates).
pub fn fetched() -> &'static str {
    &SNAPSHOT.fetched
}

/// `model` of `preset` in the snapshot. A trailing date (`-20260801`, `-2026-08-01`) is ignored
/// when the exact id isn't listed, since providers list dated ids of the same model.
pub fn lookup(preset: &str, model: &str) -> Option<&'static CatalogModel> {
    let models = SNAPSHOT.providers.get(preset)?;
    models
        .get(model)
        .or_else(|| models.get(without_date(model)?))
}

fn without_date(model: &str) -> Option<&str> {
    let (stem, last) = model.rsplit_once('-')?;
    if last.len() == 8 && last.bytes().all(|b| b.is_ascii_digit()) {
        return Some(stem);
    }
    // `-YYYY-MM-DD`: three trailing numeric parts.
    let mut parts = model.rsplitn(4, '-');
    let (day, month, year) = (parts.next()?, parts.next()?, parts.next()?);
    let stem = parts.next()?;
    let numeric =
        |part: &str, len: usize| part.len() == len && part.bytes().all(|b| b.is_ascii_digit());
    (numeric(year, 4) && numeric(month, 2) && numeric(day, 2)).then_some(stem)
}

/// What `usage` cost at `prices`, rounded up to a whole micro-USD.
pub fn cost_micro_usd(prices: Prices, usage: &Usage) -> u64 {
    let total: u128 = u128::from(usage.input_uncached) * u128::from(prices.input)
        + u128::from(usage.cache_read) * u128::from(prices.cache_read)
        + u128::from(usage.cache_write) * u128::from(prices.cache_write)
        + u128::from(usage.output) * u128::from(prices.output);
    u64::try_from(total.div_ceil(1_000_000)).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_snapshot_loads_with_the_preset_providers() {
        assert!(!fetched().is_empty());
        for preset in ["openai", "anthropic", "gemini", "openrouter"] {
            assert!(
                SNAPSHOT
                    .providers
                    .get(preset)
                    .is_some_and(|m| !m.is_empty()),
                "{preset}"
            );
        }
    }

    #[test]
    fn dated_ids_find_their_model() {
        let base = lookup("anthropic", "claude-sonnet-5").unwrap();
        assert_eq!(lookup("anthropic", "claude-sonnet-5-20260801"), Some(base));
        assert_eq!(without_date("gpt-6-luna-2026-08-01"), Some("gpt-6-luna"));
        assert_eq!(without_date("gpt-5.4-nano"), None);
        assert_eq!(lookup("anthropic", "claude-unknown-9"), None);
        assert_eq!(lookup("ollama", "qwen3.5:9b"), None);
    }

    #[test]
    fn prices_follow_the_context_tier_and_costs_round_up() {
        let model = CatalogModel {
            input: 100_000,
            output: 500_000,
            cache_read: Some(10_000),
            cache_write: None,
            tiers: vec![PriceTier {
                above: 200_000,
                input: 200_000,
                output: 750_000,
                cache_read: None,
                cache_write: None,
            }],
            context: Some(1_000_000),
            max_output: Some(128_000),
            structured_output: true,
            effort: true,
            lowest_effort: Some("none".into()),
            thinking_always_on: false,
        };
        let small = model.prices(1_000);
        assert_eq!(
            (
                small.input,
                small.output,
                small.cache_read,
                small.cache_write
            ),
            (100_000, 500_000, 10_000, 100_000)
        );
        let large = model.prices(250_000);
        assert_eq!((large.input, large.output), (200_000, 750_000));
        // 1,000 input + 100 output tokens at $0.10 / $0.50 per million = $0.00015 = 150 µUSD.
        let usage = Usage {
            input_uncached: 1_000,
            output: 100,
            ..Usage::default()
        };
        assert_eq!(cost_micro_usd(small, &usage), 150);
        let one = Usage {
            input_uncached: 1,
            ..Usage::default()
        };
        assert_eq!(cost_micro_usd(small, &one), 1, "rounded up");
        assert_eq!(cost_micro_usd(small, &Usage::default()), 0);
    }
}
