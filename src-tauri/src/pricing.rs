//! Anthropic API list prices, used to show what your Claude Code usage would
//! have cost at pay-as-you-go rates ("API value"). USD per million tokens.
//!
//! Source: anthropic.com/pricing and model launch posts, checked 2026-10-08.
//! Prices change; update `PRICES_AS_OF` and the table together.

pub const PRICES_AS_OF: &str = "2026-10-08";

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Price {
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    pub cache_write_5m: f64,
    pub cache_write_1h: f64,
    /// Some models charge more for long prompts: (threshold tokens, multiplier).
    pub long_prompt: Option<(u64, f64)>,
}

const fn p(
    input: f64,
    output: f64,
    cache_read: f64,
    cache_write_5m: f64,
    cache_write_1h: f64,
) -> Price {
    Price {
        input,
        output,
        cache_read,
        cache_write_5m,
        cache_write_1h,
        long_prompt: None,
    }
}

/// Longest prefix wins, so "claude-opus-5-5" is matched before "claude-opus-5".
const TABLE: &[(&str, Price)] = &[
    ("claude-fable-5-1", p(10.0, 50.0, 0.25, 12.5, 20.0)),
    ("claude-opus-5-5", p(4.0, 20.0, 0.20, 5.0, 8.0)),
    ("claude-opus-5", p(5.0, 25.0, 0.50, 6.25, 10.0)),
    ("claude-sonnet-5-5", p(2.0, 10.0, 0.10, 2.50, 4.0)),
    ("claude-sonnet-5", p(2.0, 10.0, 0.20, 2.50, 4.0)),
    (
        "claude-haiku-5-5",
        Price {
            input: 0.10,
            output: 0.50,
            cache_read: 0.01,
            cache_write_5m: 0.125,
            cache_write_1h: 0.20,
            long_prompt: Some((100_000, 5.0)),
        },
    ),
    ("claude-haiku-4-5", p(1.0, 5.0, 0.10, 1.25, 2.0)),
    ("claude-opus-4-5", p(5.0, 25.0, 0.50, 6.25, 10.0)),
    ("claude-opus-4", p(15.0, 75.0, 1.50, 18.75, 30.0)),
    ("claude-sonnet-4", p(3.0, 15.0, 0.30, 3.75, 6.0)),
    ("claude-3-7-sonnet", p(3.0, 15.0, 0.30, 3.75, 6.0)),
    ("claude-3-5-sonnet", p(3.0, 15.0, 0.30, 3.75, 6.0)),
    ("claude-3-5-haiku", p(0.80, 4.0, 0.08, 1.0, 1.6)),
];

pub fn price_for(model: &str) -> Option<Price> {
    TABLE
        .iter()
        .filter(|(prefix, _)| model == *prefix || model.starts_with(&format!("{prefix}-")))
        .max_by_key(|(prefix, _)| prefix.len())
        .map(|(_, price)| *price)
}

/// Token counts for one reply.
#[derive(Debug, Default, Clone, Copy)]
pub struct Tokens {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write_5m: u64,
    pub cache_write_1h: u64,
}

/// What one reply would cost at API list prices, or None for unknown models.
pub fn cost(model: &str, t: Tokens) -> Option<f64> {
    let price = price_for(model)?;
    let prompt = t.input + t.cache_read + t.cache_write_5m + t.cache_write_1h;
    let m = match price.long_prompt {
        Some((threshold, mult)) if prompt > threshold => mult,
        _ => 1.0,
    };
    let per = |tokens: u64, rate: f64| tokens as f64 * rate * m / 1_000_000.0;
    Some(
        per(t.input, price.input)
            + per(t.output, price.output)
            + per(t.cache_read, price.cache_read)
            + per(t.cache_write_5m, price.cache_write_5m)
            + per(t.cache_write_1h, price.cache_write_1h),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn longest_prefix_wins() {
        assert_eq!(price_for("claude-opus-5-5").unwrap().input, 4.0);
        assert_eq!(price_for("claude-opus-5").unwrap().input, 5.0);
        assert_eq!(price_for("claude-opus-5-20260601").unwrap().input, 5.0);
        assert_eq!(price_for("claude-sonnet-4-5-20250929").unwrap().input, 3.0);
        assert!(price_for("claude-opus-50").is_none());
        assert!(price_for("gpt-5").is_none());
    }

    #[test]
    fn prices_one_reply() {
        // 1M of each kind on Opus 5.5 = 4 + 20 + 0.20 + 5 + 8
        let m = 1_000_000;
        let t = Tokens {
            input: m,
            output: m,
            cache_read: m,
            cache_write_5m: m,
            cache_write_1h: m,
        };
        assert!((cost("claude-opus-5-5", t).unwrap() - 37.2).abs() < 1e-9);
    }

    #[test]
    fn long_haiku_prompts_cost_five_times_more() {
        let short = Tokens {
            input: 50_000,
            output: 1_000,
            ..Default::default()
        };
        let long = Tokens {
            input: 150_000,
            output: 1_000,
            ..Default::default()
        };
        let c_short = cost("claude-haiku-5-5", short).unwrap();
        let c_long = cost("claude-haiku-5-5", long).unwrap();
        assert!((c_short - (0.05 * 0.10 + 0.001 * 0.50)).abs() < 1e-12);
        assert!((c_long - 5.0 * (0.15 * 0.10 + 0.001 * 0.50)).abs() < 1e-12);
    }
}
