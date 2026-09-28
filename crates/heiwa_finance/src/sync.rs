//! One sync: brokerage, then FX, then bars, each isolated so a failing source
//! never erases what another source (or an earlier sync) already recorded.

use crate::market::{alpha_vantage, bank_of_canada};
use crate::snaptrade;
use crate::store::{FinanceStore, Snapshot, SourceIssue, SyncCounts};
use crate::FinanceError;

/// The configured sources. Any may be absent; a sync reads what it can.
#[derive(Default)]
pub struct Sources {
    pub brokerage: Option<snaptrade::Client>,
    pub fx: Option<bank_of_canada::Client>,
    pub bars: Option<alpha_vantage::Client>,
}

pub struct SyncOptions {
    /// RFC 3339 UTC time of this sync.
    pub now: String,
    /// Local `YYYY-MM-DD` for date windows.
    pub today: String,
    /// Most bar requests one sync may spend. Alpha Vantage's free tier allows
    /// 25 a day; a small cap leaves room for several syncs.
    pub max_bar_requests: usize,
    /// A bar series fetched more recently than this is not fetched again.
    pub bar_refresh_hours: i64,
    /// Problems the caller found before the sync (a credential missing from
    /// the vault, say); reported, and counted in the outcome.
    pub preflight_issues: Vec<SourceIssue>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SyncReport {
    /// `ok`, `partial`, or `error`.
    pub outcome: String,
    pub counts: SyncCounts,
    pub issues: Vec<SourceIssue>,
}

pub fn sync(
    store: &FinanceStore,
    sources: &Sources,
    options: &SyncOptions,
) -> Result<SyncReport, FinanceError> {
    let _lock = store
        .try_lock()?
        .ok_or_else(|| FinanceError::Store("a finance sync is already running".into()))?;
    let mut status = store.load_sync_status()?;
    let settings = store.load_settings()?;
    let mut counts = SyncCounts::default();
    let mut issues = options.preflight_issues.clone();
    let mut refreshed = false;

    let unconfigured =
        sources.brokerage.is_none() && sources.fx.is_none() && sources.bars.is_none();
    if unconfigured && issues.is_empty() {
        issues.push(SourceIssue {
            source: "Heiwa".into(),
            message: "No finance source is connected. Run `heiwa connect snaptrade` for brokerage accounts or `heiwa connect alpha-vantage` for market data.".into(),
        });
    }

    if let Some(client) = &sources.brokerage {
        match sync_brokerage(store, client, options, &mut counts, &mut issues) {
            Ok(()) => refreshed = true,
            Err(error) => issues.push(issue("SnapTrade", &error)),
        }
    }
    let snapshot = store.load_snapshot()?;

    if let Some(client) = &sources.fx {
        let mut currencies: Vec<String> = vec!["USD".into()];
        if let Some(snapshot) = &snapshot {
            let held = snapshot.holdings.iter().filter_map(|h| h.currency.clone());
            let cash = snapshot
                .accounts
                .iter()
                .flat_map(|a| a.cash.iter().map(|c| c.currency.clone()));
            let totals = snapshot.accounts.iter().filter_map(|a| a.currency.clone());
            currencies.extend(held.chain(cash).chain(totals));
        }
        let mut series: Vec<String> = currencies
            .iter()
            .filter_map(|code| bank_of_canada::series(code))
            .collect();
        series.sort();
        series.dedup();
        for name in series {
            let start = store
                .load_fx(&name)?
                .last()
                .and_then(|rate| shift_date(&rate.date, -7))
                .or_else(|| shift_date(&options.today, -400))
                .unwrap_or_else(|| options.today.clone());
            match client.observations(&name, &start) {
                Ok(rates) => {
                    counts.fx_days += store.merge_fx(&name, rates)?;
                    refreshed = true;
                }
                Err(error) => issues.push(issue("Bank of Canada", &error)),
            }
        }
    }

    if let Some(client) = &sources.bars {
        let mut held: Vec<String> = snapshot
            .iter()
            .flat_map(|snapshot| snapshot.holdings.iter())
            .filter(|h| {
                matches!(
                    h.kind.to_ascii_lowercase().as_str(),
                    "stock" | "etf" | "adr" | "cef"
                )
            })
            .map(|h| h.symbol.clone())
            .filter(|symbol| *symbol != settings.benchmark)
            .collect();
        held.sort();
        held.dedup();
        let mut spent = 0;
        for symbol in std::iter::once(settings.benchmark.clone()).chain(held) {
            if alpha_vantage::provider_symbol(&symbol).is_none()
                || is_fresh(
                    status.bars_fetched_at.get(&symbol),
                    &options.now,
                    options.bar_refresh_hours,
                )
            {
                continue;
            }
            if spent >= options.max_bar_requests {
                issues.push(SourceIssue {
                    source: "Alpha Vantage".into(),
                    message: "The request budget for this sync is spent; remaining series refresh on a later sync.".into(),
                });
                break;
            }
            spent += 1;
            match client.daily(&symbol) {
                Ok(bars) => {
                    counts.bars_added += store.merge_bars(&symbol, bars)?;
                    counts.bar_series += 1;
                    status.bars_fetched_at.insert(symbol, options.now.clone());
                    refreshed = true;
                }
                Err(error @ FinanceError::RateLimited { .. }) => {
                    issues.push(issue("Alpha Vantage", &error));
                    break;
                }
                Err(error) => issues.push(issue("Alpha Vantage", &error)),
            }
        }
    }

    let outcome = if issues.is_empty() {
        "ok"
    } else if refreshed {
        "partial"
    } else {
        "error"
    };
    status.last_attempt_at = Some(options.now.clone());
    if outcome != "error" {
        status.last_success_at = Some(options.now.clone());
    }
    status.outcome = Some(outcome.into());
    status.issues = issues.clone();
    status.counts = counts.clone();
    store.save_sync_status(&status)?;
    Ok(SyncReport {
        outcome: outcome.into(),
        counts,
        issues,
    })
}

/// Accounts, cash, positions, and new activities. The snapshot is replaced
/// only after the account list is read; an account whose positions fail
/// keeps its previous holdings rather than appearing empty.
fn sync_brokerage(
    store: &FinanceStore,
    client: &snaptrade::Client,
    options: &SyncOptions,
    counts: &mut SyncCounts,
    issues: &mut Vec<SourceIssue>,
) -> Result<(), FinanceError> {
    let previous = store.load_snapshot()?;
    let mut accounts = client.accounts()?;
    let stored = store.load_activities()?;
    let mut holdings = Vec::new();
    let mut positions_as_of = std::collections::BTreeMap::new();
    for account in accounts.iter_mut().filter(|account| !account.is_paper) {
        let label = account
            .number_hint
            .as_deref()
            .map(|hint| format!("account …{hint}"))
            .unwrap_or_else(|| account.name.clone());
        let carried = |previous: &Option<Snapshot>| -> Vec<crate::model::Holding> {
            previous
                .iter()
                .flat_map(|snapshot| snapshot.holdings.iter())
                .filter(|holding| holding.account_id == account.id)
                .cloned()
                .collect()
        };
        match client.balances(&account.id) {
            Ok(cash) => account.cash = cash,
            Err(error) => issues.push(issue("SnapTrade", &format!("{label} cash: {error}"))),
        }
        match client.positions(&account.id) {
            Ok((rows, as_of)) => {
                holdings.extend(rows);
                if let Some(as_of) = as_of {
                    positions_as_of.insert(account.id.clone(), as_of);
                }
            }
            Err(error) => {
                issues.push(issue(
                    "SnapTrade",
                    &format!("{label} positions (showing the last good read): {error}"),
                ));
                holdings.extend(carried(&previous));
            }
        }
        let start = stored
            .iter()
            .filter(|activity| activity.account_id == account.id)
            .filter_map(|activity| activity.trade_date.as_deref())
            .max()
            .and_then(|latest| shift_date(latest, -10));
        match client.activities(&account.id, start.as_deref()) {
            Ok(rows) => {
                let merged = store.merge_activities(rows)?;
                counts.activities_added += merged.added;
                counts.activities_updated += merged.updated;
            }
            Err(error) => issues.push(issue("SnapTrade", &format!("{label} activity: {error}"))),
        }
    }
    counts.accounts = accounts.len();
    counts.holdings = holdings.len();
    store.save_snapshot(&Snapshot {
        schema_version: crate::store::SNAPSHOT_SCHEMA,
        synced_at: options.now.clone(),
        accounts,
        holdings,
        positions_as_of,
    })
}

fn issue(source: &str, error: &dyn std::fmt::Display) -> SourceIssue {
    SourceIssue {
        source: source.into(),
        message: error.to_string(),
    }
}

/// `date` moved by `days`, as `YYYY-MM-DD`.
pub(crate) fn shift_date(date: &str, days: i64) -> Option<String> {
    Some(date_from_day_number(
        crate::analytics::day_number(date)? + days,
    ))
}

fn date_from_day_number(number: i64) -> String {
    let z = number + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

/// Seconds since the epoch for the UTC timestamps Heiwa writes
/// (`YYYY-MM-DDTHH:MM:SSZ`, optionally with fractions or `+00:00`).
fn epoch_seconds(timestamp: &str) -> Option<i64> {
    let days = crate::analytics::day_number(timestamp.get(..10)?)?;
    let rest = timestamp.get(10..)?;
    let time = rest.strip_prefix('T').or_else(|| rest.strip_prefix(' '))?;
    let zone = time.get(8..)?;
    let utc = zone.trim_start_matches(|c: char| c == '.' || c.is_ascii_digit());
    if !matches!(utc, "Z" | "+00:00" | "") {
        return None;
    }
    let hours: i64 = time.get(0..2)?.parse().ok()?;
    let minutes: i64 = time.get(3..5)?.parse().ok()?;
    let seconds: i64 = time.get(6..8)?.parse().ok()?;
    Some(days * 86_400 + hours * 3_600 + minutes * 60 + seconds)
}

fn is_fresh(fetched_at: Option<&String>, now: &str, window_hours: i64) -> bool {
    match (
        fetched_at.and_then(|at| epoch_seconds(at)),
        epoch_seconds(now),
    ) {
        (Some(at), Some(now)) => now >= at && now - at < window_hours * 3_600,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::AccountKind;
    use crate::store::SyncStatus;
    use crate::test_support::{request_line, stub};

    const ACCOUNT_ID: &str = "0a1b2c3d-0000-4000-8000-000000000001";

    fn accounts_body() -> String {
        format!(
            r#"[{{"id":"{ACCOUNT_ID}","name":"TFSA","number":"HQ1234567","institution_name":"Wealthsimple",
                 "balance":{{"total":{{"amount":560.0,"currency":"CAD"}}}},"raw_type":"TFSA","is_paper":false}}]"#
        )
    }

    fn positions_body() -> String {
        r#"{"results":[{"instrument":{"kind":"etf","id":"i1","symbol":"XEQT.TO","raw_symbol":"XEQT"},
            "units":"10","price":"36.00","cost_basis":"34.00","currency":"CAD"},
           {"instrument":{"kind":"etf","id":"i2","symbol":"SPY","raw_symbol":"SPY"},
            "units":"0.2","price":"600","currency":"USD"}],
           "data_freshness":{"as_of":"2026-09-25 14:00:00+00:00"}}"#
            .into()
    }

    fn activities_body() -> String {
        r#"{"data":[{"id":"c1","type":"CONTRIBUTION","amount":250,"currency":{"code":"CAD"},"settlement_date":"2026-08-03"},
                    {"id":"b1","type":"BUY","amount":-240,"units":7,"price":34.28,"currency":{"code":"CAD"},
                     "symbol":{"symbol":"XEQT.TO"},"trade_date":"2026-08-04 15:00:00+00:00","settlement_date":"2026-08-05"}],
            "pagination":{"offset":0,"limit":1000,"total":2}}"#
            .into()
    }

    fn daily_body(close: &str) -> String {
        format!(
            r#"{{"Time Series (Daily)":{{"2026-09-25":{{"4. close":"{close}"}},"2026-09-24":{{"4. close":"{close}"}}}}}}"#
        )
    }

    fn options() -> SyncOptions {
        SyncOptions {
            now: "2026-09-25T20:00:00Z".into(),
            today: "2026-09-25".into(),
            max_bar_requests: 8,
            bar_refresh_hours: 20,
            preflight_issues: Vec::new(),
        }
    }

    fn fixed_now() -> i64 {
        1_790_000_000
    }

    fn brokerage(url: &str) -> snaptrade::Client {
        snaptrade::Client::with_base_url(
            snaptrade::Credentials {
                client_id: "TEST-CLIENT".into(),
                consumer_key: "test-consumer-key".into(),
            },
            url,
            fixed_now,
        )
        .unwrap()
    }

    fn temp_store() -> (tempfile::TempDir, FinanceStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = FinanceStore::new(dir.path().join("finance"));
        (dir, store)
    }

    #[test]
    fn a_full_sync_records_brokerage_fx_and_bars_and_reports_ok() {
        let (_dir, store) = temp_store();
        let snaptrade_server = stub(vec![
            (200, "", accounts_body()),
            (
                200,
                "",
                r#"[{"currency":{"code":"CAD"},"cash":20.0}]"#.into(),
            ),
            (200, "", positions_body()),
            (200, "", activities_body()),
        ]);
        let valet = stub(vec![(
            200,
            "",
            r#"{"observations":[{"d":"2026-09-25","FXUSDCAD":{"v":"1.4145"}}]}"#.into(),
        )]);
        let vantage = stub(vec![
            (200, "", daily_body("40.00")),
            (200, "", daily_body("600.00")),
            (200, "", daily_body("36.00")),
        ]);
        let sources = Sources {
            brokerage: Some(brokerage(&snaptrade_server.url)),
            fx: Some(bank_of_canada::Client::with_base_url(&valet.url).unwrap()),
            bars: Some(
                alpha_vantage::Client::with_base_url("test-market-key".into(), &vantage.url)
                    .unwrap(),
            ),
        };
        let report = sync(&store, &sources, &options()).unwrap();
        snaptrade_server.thread.join().unwrap();
        valet.thread.join().unwrap();
        vantage.thread.join().unwrap();

        assert_eq!(report.outcome, "ok", "{:?}", report.issues);
        assert_eq!(report.counts.accounts, 1);
        assert_eq!(report.counts.holdings, 2);
        assert_eq!(report.counts.activities_added, 2);
        assert_eq!(report.counts.fx_days, 1);
        // The benchmark first, then every held stock or ETF.
        assert_eq!(report.counts.bar_series, 3);

        let snapshot = store.load_snapshot().unwrap().unwrap();
        assert_eq!(snapshot.accounts[0].kind, AccountKind::Tfsa);
        assert_eq!(snapshot.accounts[0].cash[0].amount, 20.0);
        assert_eq!(snapshot.holdings.len(), 2);
        assert_eq!(store.load_activities().unwrap().len(), 2);
        assert_eq!(store.load_fx("FXUSDCAD").unwrap()[0].rate, 1.4145);
        assert_eq!(store.load_bars("XIC.TO").unwrap().len(), 2);
        assert_eq!(store.load_bars("XEQT.TO").unwrap().len(), 2);

        let bar_requests = vantage.requests.lock().unwrap();
        let symbols: Vec<&str> = bar_requests
            .iter()
            .map(|raw| {
                request_line(raw)
                    .split("symbol=")
                    .nth(1)
                    .unwrap()
                    .split('&')
                    .next()
                    .unwrap()
            })
            .collect();
        assert_eq!(symbols, ["XIC.TRT", "SPY", "XEQT.TRT"]);

        let status = store.load_sync_status().unwrap();
        assert_eq!(status.outcome.as_deref(), Some("ok"));
        assert_eq!(
            status.last_success_at.as_deref(),
            Some("2026-09-25T20:00:00Z")
        );
        assert!(status.bars_fetched_at.contains_key("XIC.TO"));
    }

    #[test]
    fn a_refused_brokerage_key_keeps_the_previous_snapshot_and_says_how_to_fix_it() {
        let (_dir, store) = temp_store();
        let previous = Snapshot {
            schema_version: crate::store::SNAPSHOT_SCHEMA,
            synced_at: "2026-09-20T00:00:00Z".into(),
            accounts: Vec::new(),
            holdings: Vec::new(),
            positions_as_of: Default::default(),
        };
        store.save_snapshot(&previous).unwrap();
        let server = stub(vec![(401, "", r#"{"detail":"Invalid signature"}"#.into())]);
        let sources = Sources {
            brokerage: Some(brokerage(&server.url)),
            ..Sources::default()
        };
        let report = sync(&store, &sources, &options()).unwrap();
        server.thread.join().unwrap();
        assert_eq!(report.outcome, "error");
        assert!(report.issues[0].message.contains("heiwa connect snaptrade"));
        assert_eq!(store.load_snapshot().unwrap(), Some(previous));
        let status = store.load_sync_status().unwrap();
        assert_eq!(status.last_success_at, None);
        assert_eq!(
            status.last_attempt_at.as_deref(),
            Some("2026-09-25T20:00:00Z")
        );
    }

    #[test]
    fn bar_fetches_skip_fresh_series_and_stop_at_the_request_cap() {
        let (_dir, store) = temp_store();
        let mut status = SyncStatus::default();
        status
            .bars_fetched_at
            .insert("XIC.TO".into(), "2026-09-25T10:00:00Z".into());
        store.save_sync_status(&status).unwrap();
        let vantage = stub(vec![]);
        let sources = Sources {
            bars: Some(
                alpha_vantage::Client::with_base_url("test-market-key".into(), &vantage.url)
                    .unwrap(),
            ),
            ..Sources::default()
        };
        let report = sync(&store, &sources, &options()).unwrap();
        vantage.thread.join().unwrap();
        assert!(
            vantage.requests.lock().unwrap().is_empty(),
            "XIC.TO was fetched 10h ago"
        );
        assert_eq!(report.counts.bar_series, 0);

        let capped = SyncOptions {
            max_bar_requests: 0,
            now: "2026-09-27T20:00:00Z".into(),
            ..options()
        };
        let report = sync(&store, &sources, &capped).unwrap();
        assert_eq!(report.counts.bar_series, 0);
        assert!(report
            .issues
            .iter()
            .any(|issue| issue.message.contains("request budget")));
    }

    #[test]
    fn a_rate_limited_bar_source_stops_spending_requests_this_run() {
        let (_dir, store) = temp_store();
        store
            .save_snapshot(&Snapshot {
                schema_version: crate::store::SNAPSHOT_SCHEMA,
                synced_at: "2026-09-24T00:00:00Z".into(),
                accounts: Vec::new(),
                holdings: vec![crate::model::Holding {
                    account_id: ACCOUNT_ID.into(),
                    symbol: "XEQT.TO".into(),
                    description: None,
                    kind: "etf".into(),
                    currency: Some("CAD".into()),
                    units: 1.0,
                    price: Some(36.0),
                    average_cost: None,
                    cash_equivalent: false,
                }],
                positions_as_of: Default::default(),
            })
            .unwrap();
        let vantage = stub(vec![(
            200,
            "",
            r#"{"Information":"Our standard API rate limit is 25 requests per day."}"#.into(),
        )]);
        let sources = Sources {
            bars: Some(
                alpha_vantage::Client::with_base_url("test-market-key".into(), &vantage.url)
                    .unwrap(),
            ),
            ..Sources::default()
        };
        let report = sync(&store, &sources, &options()).unwrap();
        vantage.thread.join().unwrap();
        assert_eq!(
            vantage.requests.lock().unwrap().len(),
            1,
            "no second request after a 25/day refusal"
        );
        assert_eq!(report.outcome, "error");
        assert!(report
            .issues
            .iter()
            .any(|issue| issue.source == "Alpha Vantage"));
    }

    #[test]
    fn a_second_sync_while_one_runs_is_refused() {
        let (_dir, store) = temp_store();
        let _held = store.lock().unwrap();
        let other = FinanceStore::new(store.root());
        assert!(matches!(
            sync(&other, &Sources::default(), &options()),
            Err(FinanceError::Store(message)) if message.contains("already running")
        ));
    }

    #[test]
    fn dates_shift_across_month_and_year_boundaries() {
        assert_eq!(shift_date("2026-03-01", -1).as_deref(), Some("2026-02-28"));
        assert_eq!(shift_date("2024-03-01", -1).as_deref(), Some("2024-02-29"));
        assert_eq!(shift_date("2026-01-05", -10).as_deref(), Some("2025-12-26"));
        assert_eq!(
            shift_date("2026-09-25", -400).as_deref(),
            Some("2025-08-21")
        );
        assert_eq!(shift_date("nope", 1), None);
    }

    #[test]
    fn freshness_reads_the_timestamps_heiwa_writes() {
        let at = "2026-09-25T10:00:00Z".to_string();
        assert!(is_fresh(Some(&at), "2026-09-25T20:00:00Z", 20));
        assert!(!is_fresh(Some(&at), "2026-09-26T07:00:00Z", 20));
        assert!(is_fresh(
            Some(&"2026-09-25T10:00:00.123+00:00".to_string()),
            "2026-09-25T11:00:00Z",
            20
        ));
        assert!(!is_fresh(
            Some(&"2026-09-25T10:00:00-04:00".to_string()),
            "2026-09-25T11:00:00Z",
            20
        ));
        assert!(!is_fresh(None, "2026-09-25T11:00:00Z", 20));
    }

    #[test]
    fn preflight_issues_are_reported_instead_of_the_nothing_connected_hint() {
        let (_dir, store) = temp_store();
        let options = SyncOptions {
            preflight_issues: vec![SourceIssue {
                source: "SnapTrade".into(),
                message:
                    "the key is missing from the vault; reconnect with `heiwa connect snaptrade`"
                        .into(),
            }],
            ..options()
        };
        let report = sync(&store, &Sources::default(), &options).unwrap();
        assert_eq!(report.outcome, "error");
        assert_eq!(report.issues.len(), 1);
        assert_eq!(report.issues[0].source, "SnapTrade");
        assert_eq!(store.load_sync_status().unwrap().issues, report.issues);
    }

    #[test]
    fn a_sync_with_no_sources_explains_what_to_connect() {
        let (_dir, store) = temp_store();
        let report = sync(&store, &Sources::default(), &options()).unwrap();
        assert_eq!(report.outcome, "error");
        assert!(report.issues[0].message.contains("heiwa connect"));
    }
}
