//! `heiwa cost` — read today's totals + env rollup from `~/.heiwa/receipts.db`.
//!
//! Tokens are the primary instrument reading; cost is derived in CAD via the
//! rate table at receipt-write time. Currency presentation in other units is a
//! future concern (`--ccy USD` flag is not implemented yet).

use anyhow::{anyhow, Result};
use chrono::{Duration, Utc};
use heiwa_receipts::{Env, ReceiptStore};
use std::path::PathBuf;

pub fn run(args: &[String]) -> Result<()> {
    match args.first().map(String::as_str) {
        Some("--help") | Some("-h") => {
            print_help();
            Ok(())
        }
        _ => print_today(args),
    }
}

fn print_help() {
    println!(
        "Usage: heiwa cost [--since DURATION]\n\n\
         Reads ~/.heiwa/receipts.db and prints today's tokens, actual cost,\n\
         counterfactual cost, and a per-environment rollup.\n\n\
         Options:\n  \
           --since 1d|7d|30d   window (default: since UTC midnight)\n  \
           --help              this message"
    );
}

fn print_today(args: &[String]) -> Result<()> {
    let store_path = receipts_path()?;
    if !store_path.exists() {
        println!("HEIWA · COST · TODAY");
        println!("  no receipts recorded yet ({})", store_path.display());
        println!();
        println!("  Run an inference through the Heiwa REPL to create the first receipt.");
        return Ok(());
    }

    let store = ReceiptStore::open(&store_path).map_err(|e| anyhow!("open receipts store: {e}"))?;

    let since_unix = resolve_window(args)?;
    let total = store
        .day_total(since_unix)
        .map_err(|e| anyhow!("day_total: {e}"))?;
    let by_env = store
        .rollup_by_env(since_unix)
        .map_err(|e| anyhow!("rollup_by_env: {e}"))?;

    let savings = total.counterfactual_cost_cad - total.actual_cost_cad;
    let label = window_label(args);

    println!("HEIWA · COST · {}", label.to_uppercase());
    println!("  tokens                 {}", fmt_count(total.tokens));
    println!(
        "  actual                 {:>8.4} CAD",
        total.actual_cost_cad
    );
    println!(
        "  counterfactual         {:>8.4} CAD",
        total.counterfactual_cost_cad
    );
    println!("  savings                {:>8.4} CAD", savings);
    println!();

    if by_env.is_empty() {
        println!("  no receipts in window");
    } else {
        println!("  ENV     TOKENS     ACTUAL CAD    COUNTERFACT CAD");
        for r in by_env {
            println!(
                "  {:<7} {:>7}  {:>10.4}     {:>10.4}",
                env_label(r.env),
                fmt_count(r.tokens),
                r.actual_cost_cad,
                r.counterfactual_cost_cad,
            );
        }
    }

    println!();
    println!("  receipts: {}", store_path.display());
    Ok(())
}

fn receipts_path() -> Result<PathBuf> {
    Ok(heiwa_install::get_heiwa_dir().join("receipts.db"))
}

fn resolve_window(args: &[String]) -> Result<i64> {
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == "--since" {
            let value = iter
                .next()
                .ok_or_else(|| anyhow!("--since requires a value (e.g. 1d, 7d, 30d)"))?;
            let dur = parse_duration(value)?;
            let from = Utc::now() - dur;
            return Ok(from.timestamp());
        }
    }
    // Default: since UTC midnight today
    let midnight = Utc::now()
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| anyhow!("bad midnight calc"))?
        .and_utc();
    Ok(midnight.timestamp())
}

fn window_label(args: &[String]) -> String {
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == "--since" {
            if let Some(v) = iter.next() {
                return format!("last {v}");
            }
        }
    }
    "today".to_string()
}

fn parse_duration(s: &str) -> Result<Duration> {
    let (num, unit) = s.split_at(
        s.find(|c: char| !c.is_ascii_digit())
            .ok_or_else(|| anyhow!("duration needs unit: {s}"))?,
    );
    let n: i64 = num.parse().map_err(|_| anyhow!("not a number: {num}"))?;
    let dur = match unit {
        "d" | "day" | "days" => Duration::days(n),
        "h" | "hour" | "hours" => Duration::hours(n),
        "m" | "min" | "minutes" => Duration::minutes(n),
        other => return Err(anyhow!("unknown duration unit: {other}")),
    };
    Ok(dur)
}

fn env_label(e: Env) -> &'static str {
    match e {
        Env::Local => "local",
        Env::Oauth => "oauth",
        Env::Api => "api",
    }
}

fn fmt_count(n: i64) -> String {
    let s = n.to_string();
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(bytes.len() + bytes.len() / 3);
    for (i, &b) in bytes.iter().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            out.push(',');
        }
        out.push(b as char);
    }
    out.chars().rev().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fmt_count_inserts_thousand_separators() {
        assert_eq!(fmt_count(0), "0");
        assert_eq!(fmt_count(999), "999");
        assert_eq!(fmt_count(1_000), "1,000");
        assert_eq!(fmt_count(184_600), "184,600");
        assert_eq!(fmt_count(1_234_567), "1,234,567");
    }

    #[test]
    fn parses_duration_units() {
        assert_eq!(parse_duration("7d").unwrap(), Duration::days(7));
        assert_eq!(parse_duration("12h").unwrap(), Duration::hours(12));
        assert_eq!(parse_duration("30m").unwrap(), Duration::minutes(30));
    }

    #[test]
    fn rejects_bad_duration() {
        assert!(parse_duration("forever").is_err());
        assert!(parse_duration("12x").is_err());
    }
}
