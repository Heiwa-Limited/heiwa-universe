//! Deterministic read models over local finance truth. No network, no clock:
//! callers pass `today`, so every figure is reproducible from the store.

use crate::market::{Bar, FxRate};
use crate::model::{Account, AccountKind, Activity, ActivityKind, Holding};
use serde::Serialize;
use std::collections::BTreeMap;

/// Latest known rate into the base currency, per currency. Base is CAD: the
/// only FX source wired today publishes against the Canadian dollar.
#[derive(Debug, Clone, Default)]
pub struct FxTable {
    rates: BTreeMap<String, FxRate>,
}

impl FxTable {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record `currency`→CAD from a series, keeping its latest observation.
    pub fn insert_latest(&mut self, currency: &str, series: &[FxRate]) {
        if let Some(latest) = series.iter().max_by(|a, b| a.date.cmp(&b.date)) {
            self.rates
                .insert(currency.trim().to_ascii_uppercase(), latest.clone());
        }
    }

    /// `amount` in `currency` expressed in CAD, if a rate is known. An
    /// unlabelled amount is taken to be in the base currency already.
    pub fn to_base(&self, amount: f64, currency: Option<&str>) -> Option<f64> {
        match currency.map(|code| code.trim().to_ascii_uppercase()) {
            None => Some(amount),
            Some(code) if code == "CAD" => Some(amount),
            Some(code) => self.rates.get(&code).map(|rate| amount * rate.rate),
        }
    }

    pub fn rate(&self, currency: &str) -> Option<&FxRate> {
        self.rates.get(currency)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AccountView {
    pub id: String,
    pub name: String,
    pub institution: String,
    pub kind: AccountKind,
    pub number_hint: Option<String>,
    pub value: Option<f64>,
    pub cash: f64,
    pub positions: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PositionView {
    pub symbol: String,
    pub description: Option<String>,
    pub kind: String,
    pub currency: Option<String>,
    pub units: f64,
    pub price: Option<f64>,
    pub value_base: Option<f64>,
    pub weight: Option<f64>,
    pub average_cost: Option<f64>,
    pub unrealized_gain_base: Option<f64>,
    pub unrealized_pct: Option<f64>,
    pub accounts: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Exposure {
    pub currency: String,
    pub value_base: f64,
    pub weight: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Portfolio {
    pub base_currency: String,
    pub total_value: f64,
    pub cash: f64,
    pub invested: f64,
    pub unrealized_gain: Option<f64>,
    pub accounts: Vec<AccountView>,
    pub positions: Vec<PositionView>,
    pub currency_exposure: Vec<Exposure>,
    /// Currencies with no FX rate; their amounts are left out of totals.
    pub unconverted: Vec<String>,
}

/// Paper accounts are simulations, never part of a real total.
fn live(accounts: &[Account]) -> impl Iterator<Item = &Account> {
    accounts.iter().filter(|account| !account.is_paper)
}

fn cash_base(account: &Account, fx: &FxTable, unconverted: &mut Vec<String>) -> f64 {
    account
        .cash
        .iter()
        .filter_map(|cash| {
            let converted = fx.to_base(cash.amount, Some(&cash.currency));
            if converted.is_none() {
                unconverted.push(cash.currency.to_ascii_uppercase());
            }
            converted
        })
        .sum()
}

fn holding_value_base(
    holding: &Holding,
    fx: &FxTable,
    unconverted: &mut Vec<String>,
) -> Option<f64> {
    let value = fx.to_base(holding.market_value()?, holding.currency.as_deref());
    if value.is_none() {
        unconverted.extend(
            holding
                .currency
                .as_ref()
                .map(|code| code.to_ascii_uppercase()),
        );
    }
    value
}

/// The brokerage's reported total in CAD, or holdings plus cash when the
/// brokerage reports none.
fn account_value(
    account: &Account,
    holdings: &[Holding],
    fx: &FxTable,
    unconverted: &mut Vec<String>,
) -> Option<f64> {
    if let Some(total) = account.total_value {
        if let Some(value) = fx.to_base(total, account.currency.as_deref()) {
            return Some(value);
        }
        unconverted.extend(
            account
                .currency
                .as_ref()
                .map(|code| code.to_ascii_uppercase()),
        );
    }
    let positions: f64 = holdings
        .iter()
        .filter(|holding| holding.account_id == account.id)
        .filter_map(|holding| holding_value_base(holding, fx, unconverted))
        .sum();
    Some(positions + cash_base(account, fx, unconverted))
}

pub fn portfolio(accounts: &[Account], holdings: &[Holding], fx: &FxTable) -> Portfolio {
    let mut unconverted = Vec::new();
    let live_ids: Vec<&str> = live(accounts).map(|account| account.id.as_str()).collect();
    let holdings: Vec<&Holding> = holdings
        .iter()
        .filter(|holding| live_ids.contains(&holding.account_id.as_str()))
        .collect();

    let account_views: Vec<AccountView> = live(accounts)
        .map(|account| {
            let owned: Vec<Holding> = holdings
                .iter()
                .filter(|h| h.account_id == account.id)
                .map(|h| (*h).clone())
                .collect();
            AccountView {
                id: account.id.clone(),
                name: account.name.clone(),
                institution: account.institution.clone(),
                kind: account.kind,
                number_hint: account.number_hint.clone(),
                value: account_value(account, &owned, fx, &mut unconverted),
                cash: cash_base(account, fx, &mut unconverted),
                positions: owned.len(),
            }
        })
        .collect();
    let total_value: f64 = account_views.iter().filter_map(|view| view.value).sum();
    let cash: f64 = account_views.iter().map(|view| view.cash).sum();

    #[derive(Default)]
    struct Aggregate<'a> {
        first: Option<&'a Holding>,
        units: f64,
        value_base: Option<f64>,
        /// Book cost in the position's own currency, and in CAD.
        book_native: f64,
        book_base: Option<f64>,
        cost_complete: bool,
        price: Option<f64>,
        accounts: Vec<&'a str>,
    }
    let mut by_symbol: BTreeMap<&str, Aggregate> = BTreeMap::new();
    let mut invested = 0.0;
    let mut exposure: BTreeMap<String, f64> = BTreeMap::new();
    for holding in &holdings {
        let entry = by_symbol
            .entry(holding.symbol.as_str())
            .or_insert_with(|| Aggregate {
                cost_complete: true,
                ..Aggregate::default()
            });
        entry.first.get_or_insert(holding);
        entry.units += holding.units;
        entry.price = holding.price.or(entry.price);
        if !entry.accounts.contains(&holding.account_id.as_str()) {
            entry.accounts.push(&holding.account_id);
        }
        let value = holding_value_base(holding, fx, &mut unconverted);
        if let Some(value) = value {
            *entry.value_base.get_or_insert(0.0) += value;
            if !holding.cash_equivalent {
                invested += value;
            }
            let currency = holding
                .currency
                .clone()
                .unwrap_or_else(|| "CAD".into())
                .to_ascii_uppercase();
            *exposure.entry(currency).or_insert(0.0) += value;
        }
        let book = holding.book_value();
        match book.and_then(|book| fx.to_base(book, holding.currency.as_deref())) {
            Some(book_base) => {
                entry.book_native += book.unwrap_or_default();
                *entry.book_base.get_or_insert(0.0) += book_base;
            }
            None => entry.cost_complete = false,
        }
    }
    for account in live(accounts) {
        for cash in &account.cash {
            if let Some(value) = fx.to_base(cash.amount, Some(&cash.currency)) {
                *exposure
                    .entry(cash.currency.to_ascii_uppercase())
                    .or_insert(0.0) += value;
            }
        }
    }

    let mut positions: Vec<PositionView> = by_symbol
        .into_iter()
        .map(|(symbol, aggregate)| {
            let first = aggregate
                .first
                .expect("an aggregate always has a first holding");
            let book = aggregate.book_base.filter(|_| aggregate.cost_complete);
            let gain = aggregate
                .value_base
                .zip(book)
                .map(|(value, book)| value - book);
            PositionView {
                symbol: symbol.to_string(),
                description: first.description.clone(),
                kind: first.kind.clone(),
                currency: first.currency.clone(),
                units: aggregate.units,
                price: aggregate.price,
                value_base: aggregate.value_base,
                weight: aggregate
                    .value_base
                    .filter(|_| total_value > 0.0)
                    .map(|value| value / total_value),
                average_cost: (aggregate.cost_complete && aggregate.units != 0.0)
                    .then(|| aggregate.book_native / aggregate.units),
                unrealized_gain_base: gain,
                unrealized_pct: gain
                    .zip(book)
                    .filter(|(_, book)| *book != 0.0)
                    .map(|(gain, book)| gain / book),
                accounts: aggregate.accounts.len(),
            }
        })
        .collect();
    positions.sort_by(|a, b| {
        b.value_base
            .unwrap_or(f64::NEG_INFINITY)
            .total_cmp(&a.value_base.unwrap_or(f64::NEG_INFINITY))
            .then_with(|| a.symbol.cmp(&b.symbol))
    });
    let gains: Vec<f64> = positions
        .iter()
        .filter_map(|p| p.unrealized_gain_base)
        .collect();
    let exposure_total: f64 = exposure.values().sum();
    unconverted.sort();
    unconverted.dedup();
    Portfolio {
        base_currency: "CAD".into(),
        total_value,
        cash,
        invested,
        unrealized_gain: (!gains.is_empty()).then(|| gains.iter().sum()),
        accounts: account_views,
        positions,
        currency_exposure: exposure
            .into_iter()
            .map(|(currency, value_base)| Exposure {
                weight: if exposure_total > 0.0 {
                    value_base / exposure_total
                } else {
                    0.0
                },
                currency,
                value_base,
            })
            .collect(),
        unconverted,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TfsaReport {
    pub accounts: usize,
    pub year: i32,
    pub contributions_ytd: f64,
    pub withdrawals_ytd: f64,
    pub room_at_start: Option<f64>,
    pub room_remaining: Option<f64>,
    /// `unknown` (room not set), `ok`, `at_limit`, or `over`.
    pub room_status: String,
    pub trades_365d: usize,
    /// Sells within 30 days of a buy of the same symbol in the same account.
    pub quick_sells_365d: usize,
    /// `quiet`, `watch`, or `elevated`: a Heiwa heuristic, not a CRA rule.
    pub trading_level: String,
    pub us_listed_value: f64,
    pub notes: Vec<String>,
}

const QUICK_SELL_DAYS: i64 = 30;

fn is_canadian_listing(symbol: &str) -> bool {
    let upper = symbol.to_ascii_uppercase();
    [".TO", ".V", ".NE", ".CN"]
        .iter()
        .any(|suffix| upper.ends_with(suffix))
}

fn money(amount: f64) -> String {
    format!("${:.2}", amount)
}

/// Registered-account guardrails for TFSAs. `room_at_start` is the user's
/// January 1 room for `today`'s year from CRA My Account, when known.
pub fn tfsa_report(
    accounts: &[Account],
    holdings: &[Holding],
    activities: &[Activity],
    room_at_start: Option<f64>,
    fx: &FxTable,
    today: &str,
) -> Option<TfsaReport> {
    let tfsa: Vec<&str> = live(accounts)
        .filter(|a| a.kind == AccountKind::Tfsa)
        .map(|a| a.id.as_str())
        .collect();
    if tfsa.is_empty() {
        return None;
    }
    let year: i32 = today.get(..4)?.parse().ok()?;
    let today_number = day_number(today)?;
    let in_tfsa = |activity: &&Activity| tfsa.contains(&activity.account_id.as_str());
    let this_year = |activity: &&Activity| {
        activity
            .trade_date
            .as_deref()
            .and_then(|date| date.get(..4))
            == Some(year.to_string().as_str())
    };
    let cad = |activity: &&Activity| {
        activity
            .currency
            .as_deref()
            .is_none_or(|code| code.eq_ignore_ascii_case("CAD"))
    };
    let magnitude = |activity: &Activity| activity.amount.unwrap_or(0.0).abs();

    let mut notes = Vec::new();
    let contributions: Vec<&Activity> = activities
        .iter()
        .filter(in_tfsa)
        .filter(this_year)
        .filter(|a| a.kind == ActivityKind::Contribution)
        .collect();
    let foreign = contributions.iter().filter(|a| !cad(a)).count();
    if foreign > 0 {
        notes.push(format!(
            "{foreign} contribution(s) this year were not in CAD and are not counted; CRA measures room in CAD at the time of contribution."
        ));
    }
    let contributions_ytd: f64 = contributions
        .iter()
        .filter(|a| cad(a))
        .map(|a| magnitude(a))
        .sum();
    let withdrawals_ytd: f64 = activities
        .iter()
        .filter(in_tfsa)
        .filter(this_year)
        .filter(cad)
        .filter(|a| a.kind == ActivityKind::Withdrawal)
        .map(magnitude)
        .sum();
    let room_remaining = room_at_start.map(|room| room - contributions_ytd);
    let room_status = match room_remaining {
        None => "unknown",
        Some(remaining) if remaining < -0.005 => "over",
        Some(remaining) if remaining < 1.0 => "at_limit",
        Some(_) => "ok",
    };
    match room_remaining {
        None => notes.push(
            "Set your January 1 TFSA room from CRA My Account (`heiwa finance settings --tfsa-room <amount>`) to track what is left.".into(),
        ),
        Some(remaining) if remaining < -0.005 => notes.push(format!(
            "Contributions this year exceed the room you entered by {}; CRA charges 1% per month on the highest excess in each month until it is withdrawn.",
            money(-remaining)
        )),
        Some(_) => {}
    }
    if withdrawals_ytd > 0.0 {
        notes.push(format!(
            "Withdrawals this year ({}) are added back to your room on January 1, {}, not before.",
            money(withdrawals_ytd),
            year + 1
        ));
    }

    let dated = |activity: &Activity| activity.trade_date.as_deref().and_then(day_number);
    let trades: Vec<&Activity> = activities
        .iter()
        .filter(in_tfsa)
        .filter(|a| matches!(a.kind, ActivityKind::Buy | ActivityKind::Sell))
        .filter(|a| dated(a).is_some_and(|day| day > today_number - 365 && day <= today_number))
        .collect();
    let quick_sells_365d = trades
        .iter()
        .filter(|sell| sell.kind == ActivityKind::Sell && sell.symbol.is_some())
        .filter(|sell| {
            let sold = dated(sell).unwrap_or(i64::MIN);
            activities.iter().any(|buy| {
                buy.kind == ActivityKind::Buy
                    && buy.account_id == sell.account_id
                    && buy.symbol == sell.symbol
                    && dated(buy)
                        .is_some_and(|bought| bought <= sold && sold - bought <= QUICK_SELL_DAYS)
            })
        })
        .count();
    let trading_level = if quick_sells_365d >= 10 || trades.len() >= 100 {
        "elevated"
    } else if quick_sells_365d >= 1 {
        "watch"
    } else {
        "quiet"
    };
    if trading_level != "quiet" {
        notes.push(format!(
            "{} trades in the last 365 days; {quick_sells_365d} sold within {QUICK_SELL_DAYS} days of buying. Frequent trading can make TFSA gains taxable as business income (Ahamed v The King, 2023 TCC 17, aff'd 2024 FCA 108). This level is a Heiwa heuristic: CRA weighs frequency, holding periods, intent, knowledge, and time spent.",
            trades.len()
        ));
    }

    let mut ignored = Vec::new();
    let us_listed_value: f64 = holdings
        .iter()
        .filter(|h| tfsa.contains(&h.account_id.as_str()))
        .filter(|h| {
            h.currency
                .as_deref()
                .is_some_and(|code| code.eq_ignore_ascii_case("USD"))
        })
        .filter(|h| !is_canadian_listing(&h.symbol))
        .filter_map(|h| holding_value_base(h, fx, &mut ignored))
        .sum();
    if us_listed_value > 0.0 {
        notes.push(format!(
            "US-listed holdings worth {} are in a TFSA: the 15% US withholding tax on their dividends is not recoverable there.",
            money(us_listed_value)
        ));
    }

    Some(TfsaReport {
        accounts: tfsa.len(),
        year,
        contributions_ytd,
        withdrawals_ytd,
        room_at_start,
        room_remaining,
        room_status: room_status.into(),
        trades_365d: trades.len(),
        quick_sells_365d,
        trading_level: trading_level.into(),
        us_listed_value,
        notes,
    })
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BenchmarkReport {
    pub benchmark: String,
    /// `complete`, `partial`, `no_flows`, `no_prices`, or `unsupported_currency`.
    pub status: String,
    pub flows: usize,
    pub flows_priced: usize,
    pub net_contributed: f64,
    pub shadow_units: f64,
    pub shadow_value: Option<f64>,
    pub actual_value: Option<f64>,
    /// `actual - shadow`, only when every flow could be priced.
    pub difference: Option<f64>,
    pub first_flow: Option<String>,
    pub prices_from: Option<String>,
    pub as_of: Option<String>,
    pub notes: Vec<String>,
}

/// A flow's next close must be within this many calendar days; a longer gap
/// means the stored history is missing data, not that markets were closed.
const MAX_PRICE_GAP_DAYS: i64 = 6;

/// What the same deposits and withdrawals would be worth had each bought or
/// sold `benchmark` at the next available close. Only investment accounts
/// (registered plans and non-registered) are in scope.
pub fn benchmark_shadow(
    benchmark: &str,
    accounts: &[Account],
    holdings: &[Holding],
    activities: &[Activity],
    bars: &[Bar],
    fx: &FxTable,
) -> BenchmarkReport {
    let mut report = BenchmarkReport {
        benchmark: benchmark.into(),
        status: String::new(),
        flows: 0,
        flows_priced: 0,
        net_contributed: 0.0,
        shadow_units: 0.0,
        shadow_value: None,
        actual_value: None,
        difference: None,
        first_flow: None,
        prices_from: None,
        as_of: None,
        notes: Vec::new(),
    };
    let upper = benchmark.to_ascii_uppercase();
    if !(upper.ends_with(".TO") || upper.ends_with(".V")) {
        report.status = "unsupported_currency".into();
        report.notes.push(format!(
            "{benchmark} is not CAD-listed. Deposits are in CAD, so the comparison needs a CAD-listed benchmark such as XIC.TO."
        ));
        return report;
    }

    let in_scope: Vec<&Account> = live(accounts)
        .filter(|a| a.kind.is_registered() || a.kind == AccountKind::NonRegistered)
        .collect();
    let scope_ids: Vec<&str> = in_scope.iter().map(|a| a.id.as_str()).collect();
    let mut flows: Vec<(String, f64)> = Vec::new();
    let mut skipped_transfers = 0;
    for activity in activities
        .iter()
        .filter(|a| scope_ids.contains(&a.account_id.as_str()))
    {
        let sign = match activity.kind {
            ActivityKind::Contribution => 1.0,
            ActivityKind::Withdrawal => -1.0,
            ActivityKind::Transfer => {
                skipped_transfers += 1;
                continue;
            }
            _ => continue,
        };
        let cad = activity
            .currency
            .as_deref()
            .is_none_or(|code| code.eq_ignore_ascii_case("CAD"));
        if let (true, Some(date), Some(amount)) =
            (cad, activity.trade_date.clone(), activity.amount)
        {
            flows.push((date, sign * amount.abs()));
        }
    }
    flows.sort_by(|a, b| a.0.cmp(&b.0));
    report.flows = flows.len();
    report.first_flow = flows.first().map(|flow| flow.0.clone());
    if skipped_transfers > 0 {
        report.notes.push(format!(
            "{skipped_transfers} transfer(s) are left out: Heiwa cannot yet tell your own accounts apart from outside money in a transfer."
        ));
    }
    if flows.is_empty() {
        report.status = "no_flows".into();
        return report;
    }

    let mut bars: Vec<&Bar> = bars.iter().filter(|bar| bar.close > 0.0).collect();
    bars.sort_by(|a, b| a.date.cmp(&b.date));
    report.prices_from = bars.first().map(|bar| bar.date.clone());
    report.as_of = bars.last().map(|bar| bar.date.clone());
    for (date, amount) in &flows {
        let Some(flow_day) = day_number(date) else {
            continue;
        };
        let index = bars.partition_point(|bar| bar.date.as_str() < date.as_str());
        let Some(next) = bars.get(index) else {
            continue;
        };
        let fresh = day_number(&next.date).is_some_and(|day| day - flow_day <= MAX_PRICE_GAP_DAYS);
        if fresh {
            report.flows_priced += 1;
            report.net_contributed += amount;
            report.shadow_units += amount / next.close;
        }
    }
    let mut unconverted = Vec::new();
    let owned = |id: &str| -> Vec<Holding> {
        holdings
            .iter()
            .filter(|h| h.account_id == id)
            .cloned()
            .collect()
    };
    let actual: Option<f64> = in_scope
        .iter()
        .map(|account| account_value(account, &owned(&account.id), fx, &mut unconverted))
        .sum();
    report.actual_value = actual;
    report.status = match report.flows_priced {
        0 => "no_prices",
        priced if priced == report.flows => "complete",
        _ => "partial",
    }
    .into();
    if report.flows_priced > 0 {
        report.shadow_value = bars.last().map(|bar| report.shadow_units * bar.close);
    }
    if report.status == "complete" {
        report.difference = report
            .actual_value
            .zip(report.shadow_value)
            .map(|(a, s)| a - s);
    } else if report.flows_priced < report.flows {
        report.notes.push(format!(
            "{} of {} deposits/withdrawals have no {benchmark} close within {MAX_PRICE_GAP_DAYS} days in local history (prices start {}); there is no verdict until the history covers every flow.",
            report.flows - report.flows_priced,
            report.flows,
            report.prices_from.as_deref().unwrap_or("nowhere")
        ));
    }
    report.notes.push(format!(
        "Assumes each deposit bought {benchmark} at the next close and each withdrawal sold it, with no commission, spread, or tax. Cash you left uninvested counts against you, as it does in reality."
    ));
    report
}

/// Days since 1970-01-01 for a `YYYY-MM-DD` date (proleptic Gregorian).
pub(crate) fn day_number(date: &str) -> Option<i64> {
    if !crate::http::is_iso_date(date) {
        return None;
    }
    let year: i64 = date[0..4].parse().ok()?;
    let month: i64 = date[5..7].parse().ok()?;
    let day: i64 = date[8..10].parse().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let year_of_era = y - era * 400;
    let day_of_year = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    Some(era * 146_097 + day_of_era - 719_468)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    fn fx() -> FxTable {
        let mut table = FxTable::new();
        table.insert_latest(
            "USD",
            &[
                FxRate {
                    date: "2026-09-24".into(),
                    rate: 1.40,
                },
                FxRate {
                    date: "2026-09-25".into(),
                    rate: 1.50,
                },
            ],
        );
        table
    }

    fn account(id: &str, kind: AccountKind, total: Option<f64>) -> Account {
        Account {
            id: id.into(),
            source: "snaptrade".into(),
            institution: "Example Brokerage".into(),
            name: format!("{kind:?}"),
            number_hint: None,
            kind,
            raw_type: None,
            total_value: total,
            currency: total.map(|_| "CAD".into()),
            cash: vec![crate::model::CashBalance {
                currency: "CAD".into(),
                amount: 50.0,
            }],
            holdings_synced_at: None,
            transactions_synced_at: None,
            is_paper: false,
        }
    }

    fn holding(
        account: &str,
        symbol: &str,
        currency: &str,
        units: f64,
        price: f64,
        cost: Option<f64>,
    ) -> Holding {
        Holding {
            account_id: account.into(),
            symbol: symbol.into(),
            description: None,
            kind: "etf".into(),
            currency: Some(currency.into()),
            units,
            price: Some(price),
            average_cost: cost,
            cash_equivalent: false,
        }
    }

    fn activity(
        account: &str,
        id: &str,
        kind: ActivityKind,
        date: &str,
        amount: f64,
        symbol: Option<&str>,
    ) -> Activity {
        Activity {
            id: id.into(),
            account_id: account.into(),
            kind,
            raw_type: format!("{kind:?}").to_uppercase(),
            symbol: symbol.map(str::to_string),
            units: None,
            price: None,
            amount: Some(amount),
            currency: Some("CAD".into()),
            fee: None,
            trade_date: Some(date.into()),
            settlement_date: Some(date.into()),
            description: None,
        }
    }

    fn bar(date: &str, close: f64) -> Bar {
        Bar {
            date: date.into(),
            open: None,
            high: None,
            low: None,
            close,
            volume: None,
        }
    }

    #[test]
    fn fx_converts_into_cad_with_the_latest_rate_and_refuses_unknown_currencies() {
        let table = fx();
        assert_eq!(table.to_base(10.0, Some("CAD")), Some(10.0));
        assert_eq!(
            table.to_base(10.0, None),
            Some(10.0),
            "unlabelled amounts are base currency"
        );
        assert!(close(table.to_base(10.0, Some("USD")).unwrap(), 15.0));
        assert_eq!(table.to_base(10.0, Some("EUR")), None);
        assert_eq!(table.rate("USD").unwrap().date, "2026-09-25");
    }

    #[test]
    fn portfolio_values_accounts_in_cad_and_weights_positions_across_accounts() {
        let accounts = [
            account("tfsa", AccountKind::Tfsa, Some(1250.0)),
            account("margin", AccountKind::NonRegistered, None),
        ];
        let holdings = [
            holding("tfsa", "XIC.TO", "CAD", 20.0, 40.0, Some(35.0)),
            holding("tfsa", "VFV.TO", "CAD", 3.0, 100.0, None),
            holding("margin", "XIC.TO", "CAD", 5.0, 40.0, Some(45.0)),
            holding("margin", "SPY", "USD", 1.0, 100.0, Some(90.0)),
            holding("margin", "SAP.DE", "EUR", 1.0, 200.0, None),
        ];
        let view = portfolio(&accounts, &holdings, &fx());

        // tfsa uses the brokerage total; margin sums holdings (200 + 150) + 50 cash.
        assert!(close(view.accounts[0].value.unwrap(), 1250.0));
        assert!(close(view.accounts[1].value.unwrap(), 400.0));
        assert!(close(view.total_value, 1650.0));
        assert!(close(view.cash, 100.0));
        assert_eq!(view.unconverted, vec!["EUR".to_string()]);

        let xic = view
            .positions
            .iter()
            .find(|p| p.symbol == "XIC.TO")
            .unwrap();
        assert!(close(xic.units, 25.0));
        assert_eq!(xic.accounts, 2);
        assert!(close(xic.value_base.unwrap(), 1000.0));
        // book = 20*35 + 5*45 = 925 → average 37; gain 1000 - 925 = 75.
        assert!(close(xic.average_cost.unwrap(), 37.0));
        assert!(close(xic.unrealized_gain_base.unwrap(), 75.0));
        assert!(close(xic.weight.unwrap(), 1000.0 / 1650.0));
        assert_eq!(view.positions[0].symbol, "XIC.TO", "largest position first");

        // XIC +75, SPY (100-90)*1.5 = +15; VFV and SAP have no cost basis.
        assert!(close(view.unrealized_gain.unwrap(), 90.0));
        let usd = view
            .currency_exposure
            .iter()
            .find(|e| e.currency == "USD")
            .unwrap();
        assert!(close(usd.value_base, 150.0));
    }

    #[test]
    fn tfsa_report_tracks_this_years_contributions_against_the_users_room() {
        let accounts = [account("tfsa", AccountKind::Tfsa, Some(1000.0))];
        let activities = [
            activity(
                "tfsa",
                "c0",
                ActivityKind::Contribution,
                "2025-12-31",
                500.0,
                None,
            ),
            activity(
                "tfsa",
                "c1",
                ActivityKind::Contribution,
                "2026-02-01",
                4000.0,
                None,
            ),
            activity(
                "tfsa",
                "c2",
                ActivityKind::Contribution,
                "2026-08-01",
                3250.0,
                None,
            ),
            activity(
                "tfsa",
                "w1",
                ActivityKind::Withdrawal,
                "2026-09-01",
                -100.0,
                None,
            ),
        ];
        let report = tfsa_report(
            &accounts,
            &[],
            &activities,
            Some(7000.0),
            &fx(),
            "2026-09-25",
        )
        .unwrap();
        assert_eq!(report.year, 2026);
        assert!(close(report.contributions_ytd, 7250.0));
        assert!(close(report.withdrawals_ytd, 100.0));
        assert!(close(report.room_remaining.unwrap(), -250.0));
        assert_eq!(report.room_status, "over");
        assert!(report
            .notes
            .iter()
            .any(|note| note.contains("1% per month")));
        assert!(report
            .notes
            .iter()
            .any(|note| note.contains("January 1, 2027")));

        let unknown = tfsa_report(&accounts, &[], &activities, None, &fx(), "2026-09-25").unwrap();
        assert_eq!(unknown.room_status, "unknown");
        assert_eq!(unknown.room_remaining, None);
    }

    #[test]
    fn tfsa_report_is_absent_without_a_tfsa() {
        let accounts = [account("margin", AccountKind::NonRegistered, Some(1.0))];
        assert_eq!(
            tfsa_report(&accounts, &[], &[], None, &fx(), "2026-09-25"),
            None
        );
    }

    #[test]
    fn tfsa_report_flags_quick_round_trips_as_a_heuristic() {
        let accounts = [account("tfsa", AccountKind::Tfsa, Some(1000.0))];
        let activities = [
            activity(
                "tfsa",
                "b1",
                ActivityKind::Buy,
                "2026-05-01",
                -100.0,
                Some("SHOP.TO"),
            ),
            activity(
                "tfsa",
                "s1",
                ActivityKind::Sell,
                "2026-05-20",
                110.0,
                Some("SHOP.TO"),
            ),
            activity(
                "tfsa",
                "b2",
                ActivityKind::Buy,
                "2026-06-01",
                -100.0,
                Some("XIC.TO"),
            ),
            activity(
                "tfsa",
                "s2",
                ActivityKind::Sell,
                "2026-08-15",
                105.0,
                Some("XIC.TO"),
            ),
            activity(
                "tfsa",
                "b0",
                ActivityKind::Buy,
                "2025-01-01",
                -100.0,
                Some("SHOP.TO"),
            ),
        ];
        let report = tfsa_report(&accounts, &[], &activities, None, &fx(), "2026-09-25").unwrap();
        assert_eq!(report.trades_365d, 4);
        assert_eq!(
            report.quick_sells_365d, 1,
            "only SHOP sold within 30 days of its buy"
        );
        assert_eq!(report.trading_level, "watch");
        assert!(report
            .notes
            .iter()
            .any(|note| note.contains("2024 FCA 108")));
    }

    #[test]
    fn tfsa_report_values_us_listed_holdings_for_the_withholding_note() {
        let accounts = [account("tfsa", AccountKind::Tfsa, Some(1000.0))];
        let holdings = [
            holding("tfsa", "SPY", "USD", 2.0, 100.0, None),
            holding("tfsa", "XIC.TO", "CAD", 2.0, 40.0, None),
        ];
        let report = tfsa_report(&accounts, &holdings, &[], None, &fx(), "2026-09-25").unwrap();
        assert!(close(report.us_listed_value, 300.0));
        assert!(report.notes.iter().any(|note| note.contains("15%")));
        assert_eq!(report.trading_level, "quiet");
    }

    #[test]
    fn benchmark_shadow_buys_the_benchmark_at_the_next_close_for_every_flow() {
        let accounts = [account("tfsa", AccountKind::Tfsa, Some(560.0))];
        let activities = [
            activity(
                "tfsa",
                "c1",
                ActivityKind::Contribution,
                "2026-08-01",
                250.0,
                None,
            ),
            activity(
                "tfsa",
                "c2",
                ActivityKind::Contribution,
                "2026-09-01",
                250.0,
                None,
            ),
            activity(
                "tfsa",
                "w1",
                ActivityKind::Withdrawal,
                "2026-09-10",
                -40.0,
                None,
            ),
            activity(
                "tfsa",
                "b1",
                ActivityKind::Buy,
                "2026-08-04",
                -200.0,
                Some("SHOP.TO"),
            ),
        ];
        // Aug 1 is a Saturday: the flow buys at Monday's close.
        let bars = [
            bar("2026-07-31", 45.0),
            bar("2026-08-03", 50.0),
            bar("2026-09-01", 50.0),
            bar("2026-09-10", 40.0),
            bar("2026-09-25", 55.0),
        ];
        let report = benchmark_shadow("XIC.TO", &accounts, &[], &activities, &bars, &fx());
        assert_eq!(report.status, "complete");
        assert_eq!(report.flows, 3);
        assert_eq!(report.flows_priced, 3);
        assert!(close(report.net_contributed, 460.0));
        // 250/50 + 250/50 - 40/40 = 9 units, worth 9 * 55 = 495.
        assert!(close(report.shadow_units, 9.0));
        assert!(close(report.shadow_value.unwrap(), 495.0));
        assert!(close(report.actual_value.unwrap(), 560.0));
        assert!(close(report.difference.unwrap(), 65.0));
        assert_eq!(report.as_of.as_deref(), Some("2026-09-25"));
    }

    #[test]
    fn benchmark_shadow_withholds_a_verdict_when_history_is_incomplete() {
        let accounts = [account("tfsa", AccountKind::Tfsa, Some(560.0))];
        let activities = [
            activity(
                "tfsa",
                "c1",
                ActivityKind::Contribution,
                "2026-03-01",
                250.0,
                None,
            ),
            activity(
                "tfsa",
                "c2",
                ActivityKind::Contribution,
                "2026-09-01",
                250.0,
                None,
            ),
        ];
        let bars = [
            bar("2026-06-01", 50.0),
            bar("2026-09-02", 50.0),
            bar("2026-09-25", 55.0),
        ];
        let report = benchmark_shadow("XIC.TO", &accounts, &[], &activities, &bars, &fx());
        assert_eq!(report.status, "partial");
        assert_eq!(report.flows_priced, 1);
        assert_eq!(report.difference, None, "no verdict on a partial history");
        assert_eq!(report.prices_from.as_deref(), Some("2026-06-01"));
    }

    #[test]
    fn benchmark_shadow_never_prices_a_flow_with_a_stale_close() {
        let accounts = [account("tfsa", AccountKind::Tfsa, Some(300.0))];
        let activities = [activity(
            "tfsa",
            "c1",
            ActivityKind::Contribution,
            "2026-08-03",
            250.0,
            None,
        )];
        // The next stored close is three weeks later: a gap, not "the next close".
        let bars = [bar("2026-07-01", 50.0), bar("2026-08-24", 55.0)];
        let report = benchmark_shadow("XIC.TO", &accounts, &[], &activities, &bars, &fx());
        assert_eq!(report.flows_priced, 0);
        assert_eq!(report.status, "no_prices");
    }

    #[test]
    fn day_numbers_count_calendar_days() {
        assert_eq!(day_number("1970-01-01"), Some(0));
        assert_eq!(
            day_number("2026-03-01").unwrap() - day_number("2026-02-28").unwrap(),
            1
        );
        assert_eq!(
            day_number("2024-03-01").unwrap() - day_number("2024-02-28").unwrap(),
            2
        );
        assert_eq!(
            day_number("2027-01-01").unwrap() - day_number("2026-01-01").unwrap(),
            365
        );
        assert_eq!(day_number("2026-13-01"), None);
        assert_eq!(day_number("not-a-date"), None);
    }

    #[test]
    fn benchmark_shadow_refuses_a_benchmark_in_another_currency() {
        let accounts = [account("tfsa", AccountKind::Tfsa, Some(560.0))];
        let activities = [activity(
            "tfsa",
            "c1",
            ActivityKind::Contribution,
            "2026-08-03",
            250.0,
            None,
        )];
        let report = benchmark_shadow(
            "SPY",
            &accounts,
            &[],
            &activities,
            &[bar("2026-08-03", 500.0)],
            &fx(),
        );
        assert_eq!(report.status, "unsupported_currency");
        assert_eq!(report.shadow_value, None);
    }

    #[test]
    fn benchmark_shadow_without_flows_or_prices_says_so() {
        let accounts = [account("tfsa", AccountKind::Tfsa, Some(0.0))];
        assert_eq!(
            benchmark_shadow("XIC.TO", &accounts, &[], &[], &[], &fx()).status,
            "no_flows"
        );
        let activities = [activity(
            "tfsa",
            "c1",
            ActivityKind::Contribution,
            "2026-08-03",
            250.0,
            None,
        )];
        assert_eq!(
            benchmark_shadow("XIC.TO", &accounts, &[], &activities, &[], &fx()).status,
            "no_prices"
        );
    }
}
