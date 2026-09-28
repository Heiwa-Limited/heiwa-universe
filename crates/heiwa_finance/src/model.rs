//! Provider-neutral finance records: what Heiwa stores as local text truth.
//!
//! Amounts are `f64` in the record's own currency. These are read models for
//! display and analysis, not a ledger of record; aggregates round to cents at
//! the presentation edge.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountKind {
    Tfsa,
    Rrsp,
    Fhsa,
    Resp,
    Rrif,
    Lira,
    Crypto,
    Cash,
    NonRegistered,
    Other,
}

impl AccountKind {
    /// Canadian registered plans: contribution limits and CRA rules apply.
    pub fn is_registered(self) -> bool {
        matches!(
            self,
            Self::Tfsa | Self::Rrsp | Self::Fhsa | Self::Resp | Self::Rrif | Self::Lira
        )
    }
}

/// Classify an account from the brokerage's raw type, falling back to its
/// display name. Brokerages spell the same plan many ways (`TFSA`, `ca_tfsa`,
/// `Tax-Free Savings Account`), so this matches words, most specific first.
pub fn account_kind(raw_type: Option<&str>, name: Option<&str>) -> AccountKind {
    [raw_type, name]
        .into_iter()
        .flatten()
        .map(classify_label)
        .find(|kind| *kind != AccountKind::Other)
        .unwrap_or(AccountKind::Other)
}

fn classify_label(label: &str) -> AccountKind {
    let text = label.to_ascii_lowercase().replace(['_', '-'], " ");
    let has = |needle: &str| text.split_whitespace().any(|word| word == needle);
    if has("tfsa") || text.contains("tax free savings") {
        AccountKind::Tfsa
    } else if has("fhsa") || text.contains("first home savings") {
        AccountKind::Fhsa
    } else if has("rrsp") || has("rsp") || text.contains("retirement savings plan") {
        AccountKind::Rrsp
    } else if has("rrif") || has("rif") {
        AccountKind::Rrif
    } else if has("resp") {
        AccountKind::Resp
    } else if has("lira") || has("lif") || has("lrsp") {
        AccountKind::Lira
    } else if has("crypto") {
        AccountKind::Crypto
    } else if has("margin")
        || has("individual")
        || has("joint")
        || has("personal")
        || text.contains("non registered")
        || has("nonregistered")
    {
        AccountKind::NonRegistered
    } else if has("cash") || has("chequing") || has("savings") {
        AccountKind::Cash
    } else {
        AccountKind::Other
    }
}

/// The last four characters of an account number. Heiwa never stores the
/// full number: the hint is enough to tell a user's accounts apart.
pub fn number_hint(number: &str) -> Option<String> {
    let visible: Vec<char> = number.chars().filter(char::is_ascii_alphanumeric).collect();
    (visible.len() >= 4).then(|| visible[visible.len() - 4..].iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_kind_reads_the_brokerage_type_before_the_name() {
        assert_eq!(
            account_kind(Some("TFSA"), Some("Personal")),
            AccountKind::Tfsa
        );
        assert_eq!(account_kind(Some("ca_tfsa"), None), AccountKind::Tfsa);
        assert_eq!(account_kind(Some("ca_rrsp"), None), AccountKind::Rrsp);
        assert_eq!(account_kind(Some("Spousal RRSP"), None), AccountKind::Rrsp);
        assert_eq!(account_kind(Some("ca_fhsa"), None), AccountKind::Fhsa);
        assert_eq!(
            account_kind(Some("ca_non_registered"), None),
            AccountKind::NonRegistered
        );
        assert_eq!(
            account_kind(Some("ca_non_registered_crypto"), None),
            AccountKind::Crypto
        );
        assert_eq!(
            account_kind(Some("Margin"), None),
            AccountKind::NonRegistered
        );
        assert_eq!(account_kind(Some("ca_cash_msb"), None), AccountKind::Cash);
    }

    #[test]
    fn account_kind_falls_back_to_the_name_and_never_guesses() {
        assert_eq!(
            account_kind(None, Some("Wealthsimple Tax-Free Savings Account")),
            AccountKind::Tfsa
        );
        assert_eq!(
            account_kind(Some("unknown"), Some("TFSA")),
            AccountKind::Tfsa
        );
        assert_eq!(
            account_kind(Some("brokerage"), Some("Main")),
            AccountKind::Other
        );
        assert_eq!(account_kind(None, None), AccountKind::Other);
        // "stfsa" is not "tfsa": words, not substrings.
        assert_eq!(account_kind(Some("stfsa"), None), AccountKind::Other);
    }

    #[test]
    fn number_hint_keeps_only_the_last_four_characters() {
        assert_eq!(number_hint("HQ1234567").as_deref(), Some("4567"));
        assert_eq!(number_hint("****4567").as_deref(), Some("4567"));
        assert_eq!(number_hint("12-34"), Some("1234".to_string()));
        assert_eq!(number_hint("123"), None);
        assert_eq!(number_hint(""), None);
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CashBalance {
    pub currency: String,
    pub amount: f64,
}

/// A brokerage account as the brokerage last reported it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Account {
    pub id: String,
    /// Connector that produced the record, e.g. `snaptrade`.
    pub source: String,
    pub institution: String,
    pub name: String,
    pub number_hint: Option<String>,
    pub kind: AccountKind,
    pub raw_type: Option<String>,
    /// Total market value (cash plus positions) as reported by the brokerage.
    pub total_value: Option<f64>,
    pub currency: Option<String>,
    #[serde(default)]
    pub cash: Vec<CashBalance>,
    pub holdings_synced_at: Option<String>,
    pub transactions_synced_at: Option<String>,
    #[serde(default)]
    pub is_paper: bool,
}

/// One position in one account.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Holding {
    pub account_id: String,
    /// Yahoo-style ticker: `XEQT.TO` on the TSX, bare for US listings.
    pub symbol: String,
    pub description: Option<String>,
    /// Instrument kind reported by the source: `stock`, `etf`, `crypto`, ...
    pub kind: String,
    pub currency: Option<String>,
    pub units: f64,
    /// Last known price per unit; freshness depends on the brokerage.
    pub price: Option<f64>,
    /// Average purchase price per unit (book cost), when reported.
    pub average_cost: Option<f64>,
    #[serde(default)]
    pub cash_equivalent: bool,
}

impl Holding {
    pub fn market_value(&self) -> Option<f64> {
        self.price.map(|price| price * self.units)
    }

    pub fn book_value(&self) -> Option<f64> {
        self.average_cost.map(|cost| cost * self.units)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityKind {
    Buy,
    Sell,
    Dividend,
    Reinvest,
    Contribution,
    Withdrawal,
    Interest,
    Fee,
    Tax,
    Transfer,
    Split,
    Other,
}

/// A transaction as the brokerage recorded it. `amount` follows the source's
/// sign convention: money into the account is positive, money out negative.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Activity {
    pub id: String,
    pub account_id: String,
    pub kind: ActivityKind,
    pub raw_type: String,
    pub symbol: Option<String>,
    pub units: Option<f64>,
    pub price: Option<f64>,
    pub amount: Option<f64>,
    pub currency: Option<String>,
    pub fee: Option<f64>,
    /// `YYYY-MM-DD` as the source recorded it (UTC when the source sends a time).
    pub trade_date: Option<String>,
    pub settlement_date: Option<String>,
    pub description: Option<String>,
}
