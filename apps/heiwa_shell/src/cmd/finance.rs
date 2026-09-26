//! `heiwa finance`: the read-only finance plane on this installation.
//!
//! Brokerage accounts arrive through SnapTrade: the user creates a Personal
//! API key and links their brokerage (Wealthsimple, Questrade, ...) in
//! SnapTrade's own portal, so Heiwa never sees a brokerage password. FX comes
//! from the Bank of Canada; daily bars from Alpha Vantage with the user's key.
//! Heiwa reads. Nothing here can place a trade or move money.
//!
//! Secrets live in the OS credential vault. A non-secret enrollment file
//! records that a connector is connected, so read models never touch the
//! vault; only `connect` and `sync` do.

use anyhow::{anyhow, bail, Context, Result};
use heiwa_finance::market::{alpha_vantage, bank_of_canada};
use heiwa_finance::snaptrade;
use heiwa_finance::store::{FinanceStore, Settings, SourceIssue, TfsaRoom};
use heiwa_finance::summary::{self, Connections};
use heiwa_finance::sync::{self, Sources, SyncOptions};
use heiwa_finance::FinanceError;
use heiwa_vault::{Vault, VaultError};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::Write as _;
use std::path::PathBuf;

const KEY_SERVICE: &str = "heiwa-connector-keys";
const SNAPTRADE_KEY: &str = "snaptrade:personal";
const ALPHA_VANTAGE_KEY: &str = "alpha_vantage:default";
const ENROLLMENT_SCHEMA: &str = "heiwa_connector_enrollment_v1";
const MAX_BAR_REQUESTS_PER_SYNC: usize = 8;
const BAR_REFRESH_HOURS: i64 = 20;

pub async fn run(args: &[String]) -> Result<()> {
    match args.first().map(String::as_str) {
        None | Some("status") => status(args),
        Some("summary") => summary_command(args),
        Some("sync") => sync_command(args).await,
        Some("holdings") => holdings(args),
        Some("activity") => activity(&args[1..]),
        Some("settings") => settings(&args[1..]),
        Some("--help" | "-h" | "help") => {
            print_help();
            Ok(())
        }
        Some(other) => Err(anyhow!(
            "unknown finance command: {other} (try: heiwa finance --help)"
        )),
    }
}

fn print_help() {
    println!(
        "heiwa finance — read-only accounts and market data (Heiwa cannot trade or move money)"
    );
    println!();
    println!("Usage:");
    println!("  heiwa finance [status]                 Connections, last sync, totals");
    println!(
        "  heiwa finance summary [--json]         Full read model: portfolio, TFSA, benchmark"
    );
    println!("  heiwa finance sync [--json]            Read accounts, FX, and prices now");
    println!("  heiwa finance holdings [--json]        Positions across accounts, in CAD");
    println!("  heiwa finance activity [--limit N]     Recent transactions, newest first");
    println!("  heiwa finance settings [--benchmark SYMBOL] [--tfsa-room AMOUNT [--year YYYY]]");
    println!("                         [--clear-tfsa-room] [--json]");
    println!();
    println!("Connect:");
    println!("  heiwa connect snaptrade        SnapTrade Personal key (link Wealthsimple at dashboard.snaptrade.com)");
    println!("  heiwa connect alpha-vantage    Free Alpha Vantage key for daily prices");
}

fn store() -> FinanceStore {
    FinanceStore::new(crate::home::heiwa_state_dir().join("finance"))
}

fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|arg| arg == flag)
}

fn flag_value(args: &[String], flag: &str) -> Option<String> {
    let index = args.iter().position(|arg| arg == flag)?;
    args.get(index + 1).cloned()
}

fn finance_error(error: FinanceError) -> anyhow::Error {
    anyhow!(error.to_string())
}

// ---------------------------------------------------------------------------
// Connections: enrollment files (non-secret) + vault entries (secret)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
struct Enrollment {
    schema_version: String,
    connector: String,
    connected_at: String,
    scopes: Vec<String>,
    #[serde(default)]
    detail: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct StoredSnapTradeKey {
    client_id: String,
    consumer_key: String,
}

fn enrollment_path(connector: &str) -> PathBuf {
    crate::home::heiwa_state_dir()
        .join("connectors")
        .join(format!("{connector}.json"))
}

fn enrollment(connector: &str) -> Option<Enrollment> {
    let raw = std::fs::read(enrollment_path(connector)).ok()?;
    let enrollment: Enrollment = serde_json::from_slice(&raw).ok()?;
    (enrollment.schema_version == ENROLLMENT_SCHEMA && enrollment.connector == connector)
        .then_some(enrollment)
}

pub(crate) fn is_enrolled(connector: &str) -> bool {
    enrollment(connector).is_some()
}

fn connections() -> Connections {
    Connections {
        brokerage: is_enrolled("snaptrade"),
        market_data: is_enrolled("alpha_vantage"),
    }
}

fn enroll(connector: &str, scopes: &[&str], detail: Option<String>) -> Result<()> {
    crate::cmd::connectors::write_owner_private_json(
        &enrollment_path(connector),
        &Enrollment {
            schema_version: ENROLLMENT_SCHEMA.into(),
            connector: connector.into(),
            connected_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            scopes: scopes.iter().map(|scope| scope.to_string()).collect(),
            detail,
        },
    )
}

fn unenroll(connector: &str, vault_account: &str) -> Result<()> {
    match Vault::new(KEY_SERVICE).delete(vault_account) {
        Ok(()) | Err(VaultError::NotFound { .. }) => {}
        Err(error) => {
            return Err(anyhow!(
                "could not remove the {connector} key from the credential vault: {error}"
            ))
        }
    }
    let path = enrollment_path(connector);
    if path.exists() {
        std::fs::remove_file(&path).with_context(|| format!("remove {}", path.display()))?;
    }
    Ok(())
}

fn load_snaptrade_key() -> Result<snaptrade::Credentials> {
    let raw = Vault::new(KEY_SERVICE).load(SNAPTRADE_KEY)?;
    let stored: StoredSnapTradeKey =
        serde_json::from_str(&raw).context("stored SnapTrade key is unreadable")?;
    Ok(snaptrade::Credentials {
        client_id: stored.client_id,
        consumer_key: stored.consumer_key,
    })
}

// ---------------------------------------------------------------------------
// `heiwa connect snaptrade` / `heiwa connect alpha-vantage`
// ---------------------------------------------------------------------------

const SNAPTRADE_SCOPES: &[&str] = &[
    "finance.accounts.read",
    "finance.positions.read",
    "finance.activities.read",
];

pub(crate) async fn connect_snaptrade(args: &[String]) -> Result<()> {
    if has_flag(args, "--disconnect") {
        unenroll("snaptrade", SNAPTRADE_KEY)?;
        println!("snaptrade: disconnected; local finance read models were preserved");
        println!("  revoke the key itself at https://dashboard.snaptrade.com");
        return Ok(());
    }
    refuse_secret_flags(args, &["--consumer-key", "--key", "--secret"])?;
    let client_id = match flag_value(args, "--client-id") {
        Some(value) => value,
        None => read_line("SnapTrade client ID: ")?,
    };
    let client_id = client_id.trim().to_string();
    let well_formed = !client_id.is_empty()
        && client_id.len() <= 128
        && client_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if !well_formed {
        bail!("that does not look like a SnapTrade client ID (letters, digits, '-' and '_' only)");
    }
    let consumer_key = read_secret("SnapTrade consumer key (input hidden): ")?;
    validate_secret(&consumer_key, "SnapTrade consumer key")?;
    let credentials = snaptrade::Credentials {
        client_id,
        consumer_key,
    };

    // Verify before storing: a mistyped key should never become the saved one.
    let probe = credentials.clone();
    let accounts = tokio::task::spawn_blocking(move || snaptrade::Client::new(probe)?.accounts())
        .await
        .map_err(|_| anyhow!("SnapTrade verification stopped unexpectedly"))?
        .map_err(finance_error)?;
    let stored = serde_json::to_string(&StoredSnapTradeKey {
        client_id: credentials.client_id,
        consumer_key: credentials.consumer_key,
    })?;
    Vault::new(KEY_SERVICE)
        .store(SNAPTRADE_KEY, &stored)
        .map_err(|error| anyhow!("could not save the key to the credential vault: {error}"))?;
    let mut institutions: Vec<String> = accounts.iter().map(|a| a.institution.clone()).collect();
    institutions.sort();
    institutions.dedup();
    enroll("snaptrade", SNAPTRADE_SCOPES, Some(institutions.join(", ")))?;

    println!("snaptrade: connected, read-only");
    println!("  accounts visible: {}", accounts.len());
    if institutions.is_empty() {
        println!("  no brokerage linked yet: link Wealthsimple at https://dashboard.snaptrade.com, then sync");
    } else {
        println!("  institutions: {}", institutions.join(", "));
    }
    println!("  key: OS credential vault ({KEY_SERVICE})");
    println!("  next: heiwa finance sync");
    Ok(())
}

pub(crate) async fn connect_alpha_vantage(args: &[String]) -> Result<()> {
    if has_flag(args, "--disconnect") {
        unenroll("alpha_vantage", ALPHA_VANTAGE_KEY)?;
        println!("alpha_vantage: disconnected; stored prices were preserved");
        return Ok(());
    }
    refuse_secret_flags(args, &["--api-key", "--key"])?;
    let key = read_secret("Alpha Vantage API key (input hidden): ")?;
    validate_secret(&key, "Alpha Vantage API key")?;

    // One request (of the free tier's 25 a day) proves the key and TSX coverage.
    let probe = key.clone();
    let benchmark = store()
        .load_settings()
        .map(|s| s.benchmark)
        .unwrap_or_else(|_| "XIC.TO".into());
    let check =
        tokio::task::spawn_blocking(move || alpha_vantage::Client::new(probe)?.daily(&benchmark))
            .await
            .map_err(|_| anyhow!("Alpha Vantage verification stopped unexpectedly"))?;
    let note = match check {
        Ok(bars) => format!("verified: {} daily bars readable", bars.len()),
        Err(FinanceError::RateLimited { .. }) => {
            "saved unverified: today's Alpha Vantage quota is spent; it resets tomorrow".into()
        }
        Err(error) => return Err(finance_error(error)),
    };
    Vault::new(KEY_SERVICE)
        .store(ALPHA_VANTAGE_KEY, &key)
        .map_err(|error| anyhow!("could not save the key to the credential vault: {error}"))?;
    enroll("alpha_vantage", &["market.daily_bars.read"], None)?;
    println!("alpha_vantage: connected ({note})");
    println!("  key: OS credential vault ({KEY_SERVICE})");
    println!("  next: heiwa finance sync");
    Ok(())
}

/// Secrets on argv leak into shell history and the process table.
fn refuse_secret_flags(args: &[String], flags: &[&str]) -> Result<()> {
    let passed = args.iter().any(|arg| {
        flags
            .iter()
            .any(|flag| arg == flag || arg.starts_with(&format!("{flag}=")))
    });
    if passed {
        bail!("secrets are never accepted as arguments: paste the key at the hidden prompt, or pipe it on stdin");
    }
    Ok(())
}

fn validate_secret(value: &str, label: &str) -> Result<()> {
    if value.is_empty() {
        bail!("the {label} is empty");
    }
    if value.len() > 512 || value.chars().any(char::is_whitespace) {
        bail!("that does not look like a {label}");
    }
    Ok(())
}

/// One visible line: from the terminal after a prompt, or from stdin.
fn read_line(prompt: &str) -> Result<String> {
    use std::io::IsTerminal;
    if std::io::stdin().is_terminal() {
        eprint!("{prompt}");
        std::io::stderr().flush()?;
    }
    let mut line = String::new();
    std::io::stdin()
        .read_line(&mut line)
        .context("read from stdin")?;
    Ok(line.trim().to_string())
}

/// One secret line: typed without echo on a terminal, or read from stdin.
fn read_secret(prompt: &str) -> Result<String> {
    use std::io::IsTerminal;
    if !std::io::stdin().is_terminal() {
        return read_line(prompt);
    }
    eprint!("{prompt}");
    std::io::stderr().flush()?;
    let echo = EchoOff::new();
    let mut line = String::new();
    let read = std::io::stdin().read_line(&mut line);
    drop(echo);
    eprintln!();
    read.context("read the secret")?;
    Ok(line.trim().to_string())
}

/// Terminal echo off for the lifetime of the guard.
struct EchoOff {
    #[cfg(unix)]
    saved: Option<libc::termios>,
}

impl EchoOff {
    fn new() -> Self {
        #[cfg(unix)]
        {
            // SAFETY: tcgetattr/tcsetattr on stdin with a zeroed, then filled,
            // termios; failure leaves echo untouched and `saved` empty.
            unsafe {
                let mut current: libc::termios = std::mem::zeroed();
                if libc::tcgetattr(libc::STDIN_FILENO, &mut current) == 0 {
                    let saved = current;
                    current.c_lflag &= !libc::ECHO;
                    if libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &current) == 0 {
                        return Self { saved: Some(saved) };
                    }
                }
            }
            Self { saved: None }
        }
        #[cfg(not(unix))]
        Self {}
    }
}

impl Drop for EchoOff {
    fn drop(&mut self) {
        #[cfg(unix)]
        if let Some(saved) = self.saved {
            // SAFETY: restores the termios captured in `new`.
            unsafe {
                libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &saved);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Read model + sync (shared by the CLI and the runtime API)
// ---------------------------------------------------------------------------

fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

fn now_utc() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// `GET /api/v1/finance/summary`.
pub(crate) fn summary_payload() -> Value {
    match summary::summary(&store(), connections(), &today(), &now_utc()) {
        Ok(value) => value,
        Err(error) => json!({
            "schema_version": summary::SCHEMA,
            "policy": "read_only",
            "error": error.to_string(),
        }),
    }
}

/// One sync, then its receipt. Blocking: call from a blocking thread.
pub(crate) fn sync_blocking() -> Result<Value> {
    let mut sources = Sources::default();
    let mut preflight_issues = Vec::new();
    let mut lanes: Vec<&str> = Vec::new();
    if is_enrolled("snaptrade") {
        match load_snaptrade_key() {
            Ok(credentials) => {
                sources.brokerage = Some(snaptrade::Client::new(credentials).map_err(finance_error)?);
                lanes.push("snaptrade");
            }
            Err(error) => preflight_issues.push(SourceIssue {
                source: "SnapTrade".into(),
                message: format!("the SnapTrade key could not be read from the credential vault ({error}); reconnect with `heiwa connect snaptrade`"),
            }),
        }
    }
    if is_enrolled("alpha_vantage") {
        match Vault::new(KEY_SERVICE).load(ALPHA_VANTAGE_KEY) {
            Ok(key) => {
                sources.bars = Some(alpha_vantage::Client::new(key).map_err(finance_error)?);
                lanes.push("alpha_vantage");
            }
            Err(error) => preflight_issues.push(SourceIssue {
                source: "Alpha Vantage".into(),
                message: format!("the Alpha Vantage key could not be read from the credential vault ({error}); reconnect with `heiwa connect alpha-vantage`"),
            }),
        }
    }
    // FX only matters once there is something to value.
    if sources.brokerage.is_some() || sources.bars.is_some() {
        sources.fx = Some(bank_of_canada::Client::new().map_err(finance_error)?);
        lanes.push("bank_of_canada");
    }
    let options = SyncOptions {
        now: now_utc(),
        today: today(),
        max_bar_requests: MAX_BAR_REQUESTS_PER_SYNC,
        bar_refresh_hours: BAR_REFRESH_HOURS,
        preflight_issues,
    };
    let report = sync::sync(&store(), &sources, &options).map_err(finance_error)?;
    let receipt_id = if lanes.is_empty() {
        None
    } else {
        Some(write_sync_receipt(&report, &lanes, &options.now)?)
    };
    Ok(json!({
        "outcome": report.outcome,
        "counts": report.counts,
        "issues": report.issues,
        "receipt_id": receipt_id,
    }))
}

/// Evidence that a read happened: which lanes ran and how much changed.
/// Counts only; no amounts, symbols, or account identifiers.
fn write_sync_receipt(report: &sync::SyncReport, lanes: &[&str], at: &str) -> Result<String> {
    let receipt_id = format!("rcpt-finance-sync-{}", uuid::Uuid::new_v4().simple());
    let origin_device_id = heiwa_install::load_machine_manifest()
        .ok()
        .flatten()
        .map(|manifest| manifest.device_id);
    let issue_sources: Vec<&str> = report
        .issues
        .iter()
        .map(|issue| issue.source.as_str())
        .collect();
    let receipt = json!({
        "schema_version": "heiwa_connector_receipt_v1",
        "receipt_id": receipt_id,
        "kind": "finance_sync",
        "connector": lanes.join("+"),
        "action": "read.sync",
        "side_effect": "read",
        "origin_device_id": origin_device_id,
        "outcome": report.outcome,
        "counts": report.counts,
        "issue_sources": issue_sources,
        "created_at": at,
    });
    use heiwa_evidence::EvidenceTransport;
    heiwa_evidence::JsonlTransport::default_local()?.journal("connector_receipts", receipt)?;
    Ok(receipt_id)
}

// ---------------------------------------------------------------------------
// CLI views
// ---------------------------------------------------------------------------

fn money(value: f64) -> String {
    let negative = value < 0.0;
    let cents = (value.abs() * 100.0).round() as u64;
    let whole = (cents / 100).to_string();
    let mut grouped = String::new();
    for (index, digit) in whole.chars().enumerate() {
        if index > 0 && (whole.len() - index).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    format!(
        "{}${grouped}.{:02}",
        if negative { "-" } else { "" },
        cents % 100
    )
}

fn number(value: &Value) -> Option<f64> {
    value.as_f64()
}

fn status(args: &[String]) -> Result<()> {
    let summary = summary_payload();
    if has_flag(args, "--json") {
        println!("{summary}");
        return Ok(());
    }
    println!("finance (read-only: Heiwa cannot trade or move money)");
    let connected = |on: bool| if on { "connected" } else { "not connected" };
    println!(
        "  brokerage (SnapTrade):     {}",
        connected(summary["connections"]["brokerage"] == true)
    );
    println!(
        "  prices (Alpha Vantage):    {}",
        connected(summary["connections"]["market_data"] == true)
    );
    match summary["sync"]["last_attempt_at"].as_str() {
        Some(at) => println!(
            "  last sync:                 {at} ({})",
            summary["sync"]["outcome"].as_str().unwrap_or("unknown")
        ),
        None => println!("  last sync:                 never"),
    }
    if let Some(total) = number(&summary["portfolio"]["total_value"]) {
        let accounts = summary["portfolio"]["accounts"]
            .as_array()
            .map_or(0, Vec::len);
        println!(
            "  total:                     {} CAD across {accounts} account(s)",
            money(total)
        );
    }
    if summary["tfsa"].is_object() {
        let contributed = number(&summary["tfsa"]["contributions_ytd"]).unwrap_or(0.0);
        match number(&summary["tfsa"]["room_remaining"]) {
            Some(room) => println!(
                "  TFSA:                      {} contributed this year; {} room left",
                money(contributed),
                money(room)
            ),
            None => println!(
                "  TFSA:                      {} contributed this year; room not set",
                money(contributed)
            ),
        }
    }
    let benchmark = &summary["benchmark"];
    if benchmark.is_object() {
        let name = benchmark["benchmark"].as_str().unwrap_or("benchmark");
        match number(&benchmark["difference"]) {
            Some(difference) if difference >= 0.0 => println!(
                "  vs {name}:{:>w$}ahead by {}",
                "",
                money(difference),
                w = 22usize.saturating_sub(name.len())
            ),
            Some(difference) => println!(
                "  vs {name}:{:>w$}behind by {}",
                "",
                money(-difference),
                w = 22usize.saturating_sub(name.len())
            ),
            None => println!(
                "  vs {name}:{:>w$}{}",
                "",
                benchmark["status"].as_str().unwrap_or("unknown"),
                w = 22usize.saturating_sub(name.len())
            ),
        }
    }
    for issue in summary["sync"]["issues"].as_array().into_iter().flatten() {
        println!(
            "  issue: {}: {}",
            issue["source"].as_str().unwrap_or("?"),
            issue["message"].as_str().unwrap_or("")
        );
    }
    for action in summary["next_actions"].as_array().into_iter().flatten() {
        println!("  next: {}", action.as_str().unwrap_or(""));
    }
    Ok(())
}

fn summary_command(args: &[String]) -> Result<()> {
    let summary = summary_payload();
    if has_flag(args, "--json") {
        println!("{summary}");
    } else {
        println!("{}", serde_json::to_string_pretty(&summary)?);
    }
    Ok(())
}

async fn sync_command(args: &[String]) -> Result<()> {
    let report = tokio::task::spawn_blocking(sync_blocking)
        .await
        .map_err(|_| anyhow!("finance sync stopped unexpectedly"))??;
    if has_flag(args, "--json") {
        println!("{report}");
        return Ok(());
    }
    println!(
        "finance sync: {}",
        report["outcome"].as_str().unwrap_or("unknown")
    );
    let counts = &report["counts"];
    println!(
        "  accounts {}  holdings {}  new activity {}  fx days {}  price series {}",
        counts["accounts"],
        counts["holdings"],
        counts["activities_added"],
        counts["fx_days"],
        counts["bar_series"]
    );
    for issue in report["issues"].as_array().into_iter().flatten() {
        println!(
            "  issue: {}: {}",
            issue["source"].as_str().unwrap_or("?"),
            issue["message"].as_str().unwrap_or("")
        );
    }
    if let Some(receipt) = report["receipt_id"].as_str() {
        println!("  receipt: {receipt}");
    }
    Ok(())
}

fn holdings(args: &[String]) -> Result<()> {
    let summary = summary_payload();
    let positions = &summary["portfolio"]["positions"];
    if has_flag(args, "--json") {
        println!("{positions}");
        return Ok(());
    }
    let Some(rows) = positions.as_array().filter(|rows| !rows.is_empty()) else {
        println!("no holdings in the local snapshot (heiwa finance sync)");
        return Ok(());
    };
    println!(
        "{:<12} {:>12} {:>10} {:>14} {:>7} {:>12}",
        "SYMBOL", "UNITS", "PRICE", "VALUE (CAD)", "WEIGHT", "GAIN (CAD)"
    );
    for row in rows {
        let cell = |value: &Value, render: &dyn Fn(f64) -> String| {
            number(value).map(render).unwrap_or_else(|| "—".into())
        };
        println!(
            "{:<12} {:>12} {:>10} {:>14} {:>7} {:>12}",
            row["symbol"].as_str().unwrap_or("?"),
            cell(&row["units"], &|units| format!("{units:.4}")),
            cell(&row["price"], &|price| format!("{price:.2}")),
            cell(&row["value_base"], &money),
            cell(&row["weight"], &|weight| format!("{:.1}%", weight * 100.0)),
            cell(&row["unrealized_gain_base"], &money),
        );
    }
    Ok(())
}

fn activity(args: &[String]) -> Result<()> {
    let limit: usize = match flag_value(args, "--limit") {
        Some(raw) => raw
            .parse()
            .map_err(|_| anyhow!("--limit takes a whole number"))?,
        None => 20,
    };
    let rows = store().load_activities().map_err(finance_error)?;
    if has_flag(args, "--json") {
        let recent: Vec<_> = rows.iter().rev().take(limit).collect();
        println!("{}", serde_json::to_string(&recent)?);
        return Ok(());
    }
    if rows.is_empty() {
        println!("no transactions in the local store (heiwa finance sync)");
        return Ok(());
    }
    for row in rows.iter().rev().take(limit) {
        println!(
            "{:<10} {:<12} {:<10} {:>12} {}",
            row.trade_date.as_deref().unwrap_or("?"),
            format!("{:?}", row.kind).to_lowercase(),
            row.symbol.as_deref().unwrap_or(""),
            row.amount.map(money).unwrap_or_default(),
            row.currency.as_deref().unwrap_or(""),
        );
    }
    Ok(())
}

fn settings(args: &[String]) -> Result<()> {
    let store = store();
    let mut settings: Settings = store.load_settings().map_err(finance_error)?;
    let mut changed = false;
    if let Some(symbol) = flag_value(args, "--benchmark") {
        let symbol = symbol.trim().to_ascii_uppercase();
        let valid = !symbol.is_empty()
            && symbol.len() <= 24
            && !symbol.starts_with('.')
            && symbol
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-');
        if !valid {
            bail!("{symbol:?} is not a ticker (use the Yahoo form, e.g. XIC.TO)");
        }
        settings.benchmark = symbol;
        changed = true;
    }
    if let Some(raw) = flag_value(args, "--tfsa-room") {
        let room: f64 = raw
            .trim()
            .trim_start_matches('$')
            .replace(',', "")
            .parse()
            .map_err(|_| anyhow!("--tfsa-room takes an amount in CAD, e.g. 7000"))?;
        if !room.is_finite() || !(0.0..=1_000_000.0).contains(&room) {
            bail!("--tfsa-room must be between 0 and 1,000,000");
        }
        let year: i32 = match flag_value(args, "--year") {
            Some(raw) => raw
                .parse()
                .map_err(|_| anyhow!("--year takes a year, e.g. 2026"))?,
            None => today()[..4].parse()?,
        };
        if !(2009..=2100).contains(&year) {
            bail!("TFSAs began in 2009; --year must be 2009 or later");
        }
        settings.tfsa_room = Some(TfsaRoom {
            year,
            room_at_start: room,
        });
        changed = true;
    }
    if has_flag(args, "--clear-tfsa-room") {
        settings.tfsa_room = None;
        changed = true;
    }
    if changed {
        store.save_settings(&settings).map_err(finance_error)?;
    }
    if has_flag(args, "--json") {
        println!("{}", serde_json::to_string(&settings)?);
        return Ok(());
    }
    println!("finance settings{}", if changed { " (saved)" } else { "" });
    println!("  base currency: {}", settings.base_currency);
    println!("  benchmark:     {}", settings.benchmark);
    match &settings.tfsa_room {
        Some(room) => println!(
            "  TFSA room:     {} on January 1, {}",
            money(room.room_at_start),
            room.year
        ),
        None => println!("  TFSA room:     not set (from CRA My Account: --tfsa-room <amount>)"),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn money_groups_thousands_and_keeps_the_sign() {
        assert_eq!(money(0.0), "$0.00");
        assert_eq!(money(1234.5), "$1,234.50");
        assert_eq!(money(-1_000_000.006), "-$1,000,000.01");
        assert_eq!(money(999.999), "$1,000.00");
    }

    #[test]
    fn secret_flags_are_refused_in_both_spellings() {
        let args = |raw: &[&str]| raw.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(refuse_secret_flags(&args(&["--consumer-key", "x"]), &["--consumer-key"]).is_err());
        assert!(refuse_secret_flags(&args(&["--consumer-key=x"]), &["--consumer-key"]).is_err());
        assert!(refuse_secret_flags(&args(&["--client-id", "x"]), &["--consumer-key"]).is_ok());
    }
}
