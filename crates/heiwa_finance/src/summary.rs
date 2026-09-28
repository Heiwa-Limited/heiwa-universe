//! The finance read model every surface shares: CLI, runtime API, desktop.

use crate::analytics::{self, FxTable};
use crate::market::bank_of_canada;
use crate::store::FinanceStore;
use crate::FinanceError;
use serde_json::{json, Value};

pub const SCHEMA: &str = "heiwa_finance_summary_v1";
const RECENT_ACTIVITY: usize = 20;

/// Which credentials the caller found. The summary never reads secrets; it
/// only needs to know what is connected to say what to do next.
#[derive(Debug, Clone, Copy, Default)]
pub struct Connections {
    pub brokerage: bool,
    pub market_data: bool,
}

pub fn summary(
    store: &FinanceStore,
    connections: Connections,
    today: &str,
    generated_at: &str,
) -> Result<Value, FinanceError> {
    let settings = store.load_settings()?;
    let status = store.load_sync_status()?;
    let snapshot = store.load_snapshot()?;
    let activities = store.load_activities()?;
    let benchmark_bars = store.load_bars(&settings.benchmark).unwrap_or_default();

    let mut fx = FxTable::new();
    let mut fx_view = serde_json::Map::new();
    let mut currencies: Vec<String> = vec!["USD".into()];
    if let Some(snapshot) = &snapshot {
        currencies.extend(snapshot.holdings.iter().filter_map(|h| h.currency.clone()));
        currencies.extend(
            snapshot
                .accounts
                .iter()
                .flat_map(|a| a.cash.iter().map(|c| c.currency.clone())),
        );
    }
    currencies.sort();
    currencies.dedup();
    for currency in currencies {
        let Some(series) = bank_of_canada::series(&currency) else {
            continue;
        };
        let rates = store.load_fx(&series)?;
        fx.insert_latest(&currency, &rates);
        if let Some(rate) = fx.rate(&currency.to_ascii_uppercase()) {
            fx_view.insert(
                currency.to_ascii_uppercase(),
                json!({"rate": rate.rate, "date": rate.date, "source": "Bank of Canada"}),
            );
        }
    }

    let year: Option<i32> = today.get(..4).and_then(|year| year.parse().ok());
    let room = settings
        .tfsa_room
        .as_ref()
        .filter(|room| Some(room.year) == year)
        .map(|room| room.room_at_start);

    let (portfolio, tfsa, benchmark, recent) = match &snapshot {
        None => (Value::Null, Value::Null, Value::Null, Vec::new()),
        Some(snapshot) => {
            let portfolio = analytics::portfolio(&snapshot.accounts, &snapshot.holdings, &fx);
            let tfsa = analytics::tfsa_report(
                &snapshot.accounts,
                &snapshot.holdings,
                &activities,
                room,
                &fx,
                today,
            );
            let benchmark = analytics::benchmark_shadow(
                &settings.benchmark,
                &snapshot.accounts,
                &snapshot.holdings,
                &activities,
                &benchmark_bars,
                &fx,
            );
            let label = |account_id: &str| -> String {
                snapshot
                    .accounts
                    .iter()
                    .find(|account| account.id == account_id)
                    .map(|account| match &account.number_hint {
                        Some(hint) => format!("{} …{hint}", account.name),
                        None => account.name.clone(),
                    })
                    .unwrap_or_else(|| "Unknown account".into())
            };
            let recent: Vec<Value> = activities
                .iter()
                .rev()
                .take(RECENT_ACTIVITY)
                .map(|activity| {
                    json!({
                        "date": activity.trade_date,
                        "kind": activity.kind,
                        "symbol": activity.symbol,
                        "units": activity.units,
                        "amount": activity.amount,
                        "currency": activity.currency,
                        "account": label(&activity.account_id),
                        "description": activity.description,
                    })
                })
                .collect();
            (
                serde_json::to_value(portfolio).unwrap_or(Value::Null),
                tfsa.map(|report| serde_json::to_value(report).unwrap_or(Value::Null))
                    .unwrap_or(Value::Null),
                serde_json::to_value(benchmark).unwrap_or(Value::Null),
                recent,
            )
        }
    };

    let mut next_actions: Vec<String> = Vec::new();
    if !connections.brokerage {
        next_actions.push(
            "Connect your brokerage read-only: create a free SnapTrade Personal key at https://dashboard.snaptrade.com, link Wealthsimple there, then run `heiwa connect snaptrade`.".into(),
        );
    }
    if !connections.market_data {
        next_actions.push(format!(
            "Add a free Alpha Vantage key with `heiwa connect alpha-vantage` to measure your account against {}.",
            settings.benchmark
        ));
    }
    let never_synced = status.last_attempt_at.is_none() && status.last_success_at.is_none();
    if (connections.brokerage || connections.market_data) && never_synced {
        next_actions.push("Run `heiwa finance sync` to read your accounts and prices.".into());
    }
    if tfsa.get("room_status").and_then(Value::as_str) == Some("unknown") {
        next_actions.push(
            "Enter your January 1 TFSA room from CRA My Account: `heiwa finance settings --tfsa-room <amount>`.".into(),
        );
    }

    let oldest_positions = snapshot
        .as_ref()
        .and_then(|snapshot| snapshot.positions_as_of.values().min().cloned());
    Ok(json!({
        "schema_version": SCHEMA,
        "generated_at": generated_at,
        "policy": "read_only",
        "policy_note": "Heiwa reads balances, positions, and transactions. It has no way to place a trade or move money.",
        "connections": {"brokerage": connections.brokerage, "market_data": connections.market_data},
        "settings": {
            "base_currency": settings.base_currency,
            "benchmark": settings.benchmark,
            "tfsa_room": settings.tfsa_room,
        },
        "sync": status,
        "fx": fx_view,
        "portfolio": portfolio,
        "tfsa": tfsa,
        "benchmark": benchmark,
        "activity": {"recent": recent, "total": activities.len()},
        "freshness": {
            "snapshot_synced_at": snapshot.as_ref().map(|snapshot| snapshot.synced_at.clone()),
            "positions_as_of_oldest": oldest_positions,
            "benchmark_bars_through": benchmark_bars.last().map(|bar| bar.date.clone()),
        },
        "next_actions": next_actions,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::market::{Bar, FxRate};
    use crate::model::{Account, AccountKind, Activity, ActivityKind, CashBalance, Holding};
    use crate::store::{Settings, Snapshot, SyncStatus, TfsaRoom, SNAPSHOT_SCHEMA};

    fn temp_store() -> (tempfile::TempDir, FinanceStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = FinanceStore::new(dir.path().join("finance"));
        (dir, store)
    }

    fn seed(store: &FinanceStore) {
        store
            .save_snapshot(&Snapshot {
                schema_version: SNAPSHOT_SCHEMA,
                synced_at: "2026-09-25T20:00:00Z".into(),
                accounts: vec![Account {
                    id: "tfsa".into(),
                    source: "snaptrade".into(),
                    institution: "Wealthsimple".into(),
                    name: "TFSA".into(),
                    number_hint: Some("4567".into()),
                    kind: AccountKind::Tfsa,
                    raw_type: Some("TFSA".into()),
                    total_value: Some(560.0),
                    currency: Some("CAD".into()),
                    cash: vec![CashBalance {
                        currency: "CAD".into(),
                        amount: 20.0,
                    }],
                    holdings_synced_at: None,
                    transactions_synced_at: None,
                    is_paper: false,
                }],
                holdings: vec![Holding {
                    account_id: "tfsa".into(),
                    symbol: "XEQT.TO".into(),
                    description: Some("iShares Core Equity ETF Portfolio".into()),
                    kind: "etf".into(),
                    currency: Some("CAD".into()),
                    units: 15.0,
                    price: Some(36.0),
                    average_cost: Some(33.0),
                    cash_equivalent: false,
                }],
                positions_as_of: [("tfsa".to_string(), "2026-09-25 14:00:00+00:00".to_string())]
                    .into(),
            })
            .unwrap();
        let contribution = |id: &str, date: &str| Activity {
            id: id.into(),
            account_id: "tfsa".into(),
            kind: ActivityKind::Contribution,
            raw_type: "CONTRIBUTION".into(),
            symbol: None,
            units: None,
            price: None,
            amount: Some(250.0),
            currency: Some("CAD".into()),
            fee: None,
            trade_date: Some(date.into()),
            settlement_date: Some(date.into()),
            description: None,
        };
        store
            .merge_activities(vec![
                contribution("c1", "2026-08-03"),
                contribution("c2", "2026-09-01"),
            ])
            .unwrap();
        store
            .merge_fx(
                "FXUSDCAD",
                vec![FxRate {
                    date: "2026-09-25".into(),
                    rate: 1.4145,
                }],
            )
            .unwrap();
        let bar = |date: &str, close: f64| Bar {
            date: date.into(),
            open: None,
            high: None,
            low: None,
            close,
            volume: None,
        };
        store
            .merge_bars(
                "XIC.TO",
                vec![
                    bar("2026-08-03", 50.0),
                    bar("2026-09-01", 50.0),
                    bar("2026-09-25", 52.0),
                ],
            )
            .unwrap();
        store
            .save_settings(&Settings {
                tfsa_room: Some(TfsaRoom {
                    year: 2026,
                    room_at_start: 7000.0,
                }),
                ..Settings::default()
            })
            .unwrap();
        store
            .save_sync_status(&SyncStatus {
                last_success_at: Some("2026-09-25T20:00:00Z".into()),
                outcome: Some("ok".into()),
                ..SyncStatus::default()
            })
            .unwrap();
    }

    #[test]
    fn an_empty_store_says_what_to_connect_and_claims_nothing() {
        let (_dir, store) = temp_store();
        let value = summary(
            &store,
            Connections::default(),
            "2026-09-25",
            "2026-09-25T20:00:00Z",
        )
        .unwrap();
        assert_eq!(value["schema_version"], SCHEMA);
        assert_eq!(value["policy"], "read_only");
        assert!(value["portfolio"].is_null());
        assert!(value["tfsa"].is_null());
        let actions = value["next_actions"].to_string();
        assert!(actions.contains("heiwa connect snaptrade"), "{actions}");
        assert!(actions.contains("heiwa connect alpha-vantage"), "{actions}");
    }

    #[test]
    fn a_seeded_store_composes_portfolio_tfsa_benchmark_and_recent_activity() {
        let (_dir, store) = temp_store();
        seed(&store);
        let value = summary(
            &store,
            Connections {
                brokerage: true,
                market_data: true,
            },
            "2026-09-25",
            "2026-09-25T21:00:00Z",
        )
        .unwrap();
        assert_eq!(value["portfolio"]["total_value"], 560.0);
        assert_eq!(value["portfolio"]["positions"][0]["symbol"], "XEQT.TO");
        assert_eq!(value["tfsa"]["contributions_ytd"], 500.0);
        assert_eq!(value["tfsa"]["room_remaining"], 6500.0);
        assert_eq!(value["benchmark"]["benchmark"], "XIC.TO");
        assert_eq!(value["benchmark"]["status"], "complete");
        // 10 units of XIC at 52 = 520 vs an actual 560.
        assert_eq!(value["benchmark"]["shadow_value"], 520.0);
        assert_eq!(value["benchmark"]["difference"], 40.0);
        assert_eq!(value["fx"]["USD"]["rate"], 1.4145);
        assert_eq!(value["fx"]["USD"]["source"], "Bank of Canada");
        let recent = value["activity"]["recent"].as_array().unwrap();
        assert_eq!(recent[0]["date"], "2026-09-01", "newest first");
        assert_eq!(recent[0]["account"], "TFSA …4567");
        assert_eq!(
            value["freshness"]["snapshot_synced_at"],
            "2026-09-25T20:00:00Z"
        );
        assert_eq!(value["freshness"]["benchmark_bars_through"], "2026-09-25");
        assert_eq!(value["sync"]["outcome"], "ok");
        assert!(value["next_actions"].as_array().unwrap().is_empty());
    }

    #[test]
    fn tfsa_room_from_another_year_is_not_applied() {
        let (_dir, store) = temp_store();
        seed(&store);
        store
            .save_settings(&Settings {
                tfsa_room: Some(TfsaRoom {
                    year: 2025,
                    room_at_start: 7000.0,
                }),
                ..Settings::default()
            })
            .unwrap();
        let value = summary(
            &store,
            Connections {
                brokerage: true,
                market_data: true,
            },
            "2026-09-25",
            "2026-09-25T21:00:00Z",
        )
        .unwrap();
        assert_eq!(value["tfsa"]["room_status"], "unknown");
        assert!(value["next_actions"].to_string().contains("--tfsa-room"));
    }
}
