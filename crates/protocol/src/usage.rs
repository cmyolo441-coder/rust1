//! Token + cost accounting. Cost is integer micro-USD — floats in money are a bug.

use serde::{Deserialize, Serialize};
use std::ops::Add;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    #[serde(default)]
    pub input: u64,
    #[serde(default)]
    pub output: u64,
    #[serde(default)]
    pub cache_read: u64,
    #[serde(default)]
    pub cache_write: u64,
    #[serde(default)]
    pub reasoning: u64,
    /// Micro-USD (1e-6 USD) accumulated for this usage record.
    #[serde(default)]
    pub cost_micros: u64,
}

impl Usage {
    pub fn new(input: u64, output: u64) -> Self {
        Self {
            input,
            output,
            ..Default::default()
        }
    }

    /// Everything the provider bills you for, cache tokens included.
    pub fn billable_input(&self) -> u64 {
        self.input + self.cache_read + self.cache_write
    }

    pub fn total_tokens(&self) -> u64 {
        self.billable_input() + self.output + self.reasoning
    }

    /// Cache effectiveness, 0.0..=1.0. Used by the status bar and by budget alerts.
    pub fn cache_hit_rate(&self) -> f32 {
        let denom = self.input + self.cache_read;
        if denom == 0 {
            0.0
        } else {
            self.cache_read as f32 / denom as f32
        }
    }

    pub fn saturating_add_assign(&mut self, other: &Usage) {
        self.input = self.input.saturating_add(other.input);
        self.output = self.output.saturating_add(other.output);
        self.cache_read = self.cache_read.saturating_add(other.cache_read);
        self.cache_write = self.cache_write.saturating_add(other.cache_write);
        self.reasoning = self.reasoning.saturating_add(other.reasoning);
        self.cost_micros = self.cost_micros.saturating_add(other.cost_micros);
    }
}

impl Add for Usage {
    type Output = Usage;
    fn add(mut self, rhs: Usage) -> Usage {
        self.saturating_add_assign(&rhs);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_aggregates_without_overflow_panic() {
        let mut a = Usage::new(10, 5);
        a.cache_read = 90;
        let b = Usage::new(10, 5);
        let c = a + b;
        assert_eq!(c.input, 20);
        assert_eq!(c.output, 10);
        assert_eq!(c.cache_read, 90);
        assert_eq!(c.billable_input(), 110);
        assert_eq!(c.total_tokens(), 120);
        assert!((c.cache_hit_rate() - 90.0 / 110.0).abs() < 1e-6);
    }
}
