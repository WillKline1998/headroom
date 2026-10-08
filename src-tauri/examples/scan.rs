//! Prints what the Models tab would show, straight from this computer's logs.
//! `cargo run --example scan` (from src-tauri/)

use std::collections::BTreeMap;

fn main() {
    let a = headroom_lib::analytics::scan();
    println!(
        "{} log files, {} sessions, sources: {:?}",
        a.files_scanned, a.sessions, a.sources
    );
    let mut by_model: BTreeMap<&str, (u64, u64, f64, u64)> = BTreeMap::new();
    for d in &a.days {
        let e = by_model.entry(&d.model).or_default();
        e.0 += d.replies;
        e.1 += d.output_tokens;
        e.2 += d.api_value;
        e.3 += d.unpriced_replies;
    }
    for (model, (replies, out, value, unpriced)) in &by_model {
        println!(
            "{model:28} {replies:6} replies {out:>10} out  ${value:>9.2}  unpriced {unpriced}"
        );
    }
    let total: f64 = by_model.values().map(|v| v.2).sum();
    println!(
        "API value, all time: ${total:.2} (prices as of {})",
        a.prices_as_of
    );
    let mut hours = [0u64; 24];
    for h in &a.hours {
        hours[h.hour as usize] += h.replies;
    }
    let busiest = (0..24).max_by_key(|&h| hours[h]).unwrap_or(0);
    println!("busiest hour: {busiest}:00 ({} replies)", hours[busiest]);
}
