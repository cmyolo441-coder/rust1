//! Model descriptors: adding a model is a data change, not a code change.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelCaps {
    #[serde(default)]
    pub tools: bool,
    #[serde(default)]
    pub parallel_tools: bool,
    #[serde(default)]
    pub vision: bool,
    #[serde(default)]
    pub thinking: bool,
    #[serde(default)]
    pub prompt_cache: bool,
    #[serde(default)]
    pub streaming: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Price {
    /// Micro-USD per million input tokens.
    #[serde(default)]
    pub input_micros_per_mtok: u64,
    /// Micro-USD per million output tokens.
    #[serde(default)]
    pub output_micros_per_mtok: u64,
    /// Micro-USD per million cached-read tokens (usually 10% of input).
    #[serde(default)]
    pub cache_read_micros_per_mtok: u64,
}

impl Price {
    /// Cost of a usage record in micro-USD, rounded down (never over-charge).
    pub fn cost_micros(&self, usage: &crate::usage::Usage) -> u64 {
        let per = |tokens: u64, rate: u64| -> u64 {
            (tokens as u128 * rate as u128 / 1_000_000) as u64
        };
        per(usage.input, self.input_micros_per_mtok)
            + per(usage.output, self.output_micros_per_mtok)
            + per(usage.cache_read, self.cache_read_micros_per_mtok)
            + per(usage.cache_write, self.input_micros_per_mtok)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelDescriptor {
    /// Fully qualified id: `openai/gpt-5`, `anthropic/claude-sonnet-4-5`.
    pub id: String,
    pub provider: String,
    #[serde(default = "default_ctx")]
    pub ctx_window: u32,
    #[serde(default = "default_max_out")]
    pub max_output: u32,
    #[serde(default)]
    pub caps: ModelCaps,
    #[serde(default)]
    pub price: Price,
}

fn default_ctx() -> u32 {
    200_000
}
fn default_max_out() -> u32 {
    16_000
}

impl ModelDescriptor {
    pub fn new(id: impl Into<String>, provider: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            provider: provider.into(),
            ctx_window: default_ctx(),
            max_output: default_max_out(),
            caps: ModelCaps {
                tools: true,
                streaming: true,
                ..Default::default()
            },
            price: Price::default(),
        }
    }

    /// `provider/model` → (provider, model). A bare model name keeps the provider empty.
    pub fn split_id(id: &str) -> (Option<&str>, &str) {
        match id.split_once('/') {
            Some((p, m)) if !p.is_empty() && !m.is_empty() => (Some(p), m),
            _ => (None, id),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::Usage;

    #[test]
    fn split_id_handles_both_forms() {
        assert_eq!(
            ModelDescriptor::split_id("anthropic/claude-sonnet-4-5"),
            (Some("anthropic"), "claude-sonnet-4-5")
        );
        assert_eq!(ModelDescriptor::split_id("gpt-5"), (None, "gpt-5"));
        assert_eq!(
            ModelDescriptor::split_id("openrouter/meta/llama"),
            (Some("openrouter"), "meta/llama")
        );
    }

    #[test]
    fn price_is_integer_micros() {
        let p = Price {
            input_micros_per_mtok: 3_000_000,  // $3/Mtok
            output_micros_per_mtok: 15_000_000, // $15/Mtok
            cache_read_micros_per_mtok: 300_000,
        };
        let u = Usage {
            input: 1_000_000,
            output: 1_000_000,
            cache_read: 1_000_000,
            ..Default::default()
        };
        assert_eq!(p.cost_micros(&u), 3_000_000 + 15_000_000 + 300_000);
    }
}
