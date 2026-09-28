//! SnapTrade Personal API client (read-only).

use crate::model::{self, Account, Activity, ActivityKind, CashBalance, Holding};
use crate::FinanceError;
use base64::Engine as _;
use hmac::{Hmac, Mac};
use sha2::Sha256;

const SOURCE: &str = "SnapTrade";

/// Normalize `GET /accounts`.
pub fn parse_accounts(body: &str) -> Result<Vec<Account>, FinanceError> {
    let rows: Vec<wire::Account> = decode(body)?;
    Ok(rows
        .into_iter()
        .map(|row| {
            let institution = non_empty(row.institution_name).unwrap_or_else(|| "Brokerage".into());
            let name = non_empty(row.name).unwrap_or_else(|| format!("{institution} account"));
            let sync = row.sync_status.unwrap_or_default();
            let total = row.balance.and_then(|balance| balance.total);
            Account {
                kind: model::account_kind(row.raw_type.as_deref(), Some(&name)),
                id: row.id,
                source: "snaptrade".into(),
                number_hint: row.number.as_deref().and_then(model::number_hint),
                raw_type: row.raw_type,
                total_value: total.as_ref().and_then(|money| money.amount),
                currency: total.and_then(|money| money.currency),
                cash: Vec::new(),
                holdings_synced_at: sync.holdings.and_then(|lane| lane.last_successful_sync),
                transactions_synced_at: sync
                    .transactions
                    .and_then(|lane| lane.last_successful_sync),
                is_paper: row.is_paper.unwrap_or(false),
                institution,
                name,
            }
        })
        .collect())
}

/// Normalize `GET /accounts/{id}/balances` into per-currency cash. A row
/// without a currency cannot be attributed and is dropped.
pub fn parse_balances(body: &str) -> Result<Vec<CashBalance>, FinanceError> {
    let rows: Vec<wire::Balance> = decode(body)?;
    Ok(rows
        .into_iter()
        .filter_map(|row| {
            let currency = non_empty(row.currency.and_then(|currency| currency.code))?;
            Some(CashBalance {
                currency,
                amount: row.cash.unwrap_or(0.0),
            })
        })
        .collect())
}

/// Normalize `GET /accounts/{id}/positions/all`; returns holdings and the
/// source's `as_of` time. A position without a unit count is skipped rather
/// than guessed.
pub fn parse_positions(
    account_id: &str,
    body: &str,
) -> Result<(Vec<Holding>, Option<String>), FinanceError> {
    let response: wire::Positions = decode(body)?;
    let holdings = response
        .results
        .into_iter()
        .filter_map(|row| {
            Some(Holding {
                account_id: account_id.to_string(),
                units: row.units?,
                symbol: row.instrument.symbol,
                description: non_empty(row.instrument.description),
                kind: non_empty(row.instrument.kind).unwrap_or_else(|| "other".into()),
                currency: non_empty(row.currency).or(non_empty(row.instrument.currency)),
                price: row.price,
                average_cost: row.cost_basis,
                cash_equivalent: row.cash_equivalent.unwrap_or(false),
            })
        })
        .collect();
    Ok((
        holdings,
        response
            .data_freshness
            .and_then(|freshness| freshness.as_of),
    ))
}

/// One page of `GET /accounts/{id}/activities`.
#[derive(Debug)]
pub struct ActivityPage {
    pub activities: Vec<Activity>,
    pub total: Option<u64>,
}

pub fn parse_activities(account_id: &str, body: &str) -> Result<ActivityPage, FinanceError> {
    let page: wire::ActivityPage = decode(body)?;
    let activities = page
        .data
        .into_iter()
        .map(|row| {
            let raw_type = row.kind.unwrap_or_default();
            let settlement_date = row.settlement_date.as_deref().and_then(date_part);
            Activity {
                id: row.id,
                account_id: account_id.to_string(),
                kind: activity_kind(&raw_type),
                raw_type,
                symbol: non_empty(row.symbol.and_then(|symbol| symbol.symbol)),
                units: row.units,
                price: row.price,
                amount: row.amount,
                currency: non_empty(row.currency.and_then(|currency| currency.code)),
                fee: row.fee,
                trade_date: row
                    .trade_date
                    .as_deref()
                    .and_then(date_part)
                    .or_else(|| settlement_date.clone()),
                settlement_date,
                description: non_empty(row.description),
            }
        })
        .collect();
    Ok(ActivityPage {
        activities,
        total: page.pagination.and_then(|p| p.total),
    })
}

/// SnapTrade's normalized transaction types onto Heiwa's smaller vocabulary.
pub fn activity_kind(raw_type: &str) -> ActivityKind {
    match raw_type.trim().to_ascii_uppercase().as_str() {
        "BUY" => ActivityKind::Buy,
        "SELL" => ActivityKind::Sell,
        "DIVIDEND" | "SUBSTITUTE_DIVIDEND" | "STOCK_DIVIDEND" => ActivityKind::Dividend,
        "REI" => ActivityKind::Reinvest,
        "CONTRIBUTION" => ActivityKind::Contribution,
        "WITHDRAWAL" => ActivityKind::Withdrawal,
        "INTEREST" => ActivityKind::Interest,
        "FEE" => ActivityKind::Fee,
        "TAX" => ActivityKind::Tax,
        "TRANSFER" | "EXTERNAL_ASSET_TRANSFER_IN" | "EXTERNAL_ASSET_TRANSFER_OUT" => {
            ActivityKind::Transfer
        }
        "SPLIT" => ActivityKind::Split,
        _ => ActivityKind::Other,
    }
}

fn decode<T: serde::de::DeserializeOwned>(body: &str) -> Result<T, FinanceError> {
    serde_json::from_str(body).map_err(|error| FinanceError::Decode {
        source_name: SOURCE.into(),
        detail: error.to_string(),
    })
}

fn non_empty(value: Option<String>) -> Option<String> {
    value
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
}

/// `YYYY-MM-DD` from a date or timestamp in any of the shapes SnapTrade
/// sends (`2026-08-03`, `2026-08-03T14:05:00Z`, `2026-08-03 14:05:00+00:00`).
pub(crate) fn date_part(value: &str) -> Option<String> {
    let date = value.trim().get(..10)?;
    crate::http::is_iso_date(date).then(|| date.to_string())
}

/// Wire shapes. Every field the schema allows to be absent or null is
/// optional here, unknown fields are ignored, and numbers are accepted as
/// JSON numbers or decimal strings (positions send strings, activities send
/// numbers).
mod wire {
    use serde::{Deserialize, Deserializer};

    #[derive(Deserialize)]
    pub struct Account {
        pub id: String,
        #[serde(default)]
        pub name: Option<String>,
        #[serde(default)]
        pub number: Option<String>,
        #[serde(default)]
        pub institution_name: Option<String>,
        #[serde(default)]
        pub sync_status: Option<SyncStatus>,
        #[serde(default)]
        pub balance: Option<AccountBalance>,
        #[serde(default)]
        pub raw_type: Option<String>,
        #[serde(default)]
        pub is_paper: Option<bool>,
    }

    #[derive(Deserialize, Default)]
    pub struct SyncStatus {
        #[serde(default)]
        pub holdings: Option<SyncLane>,
        #[serde(default)]
        pub transactions: Option<SyncLane>,
    }

    #[derive(Deserialize)]
    pub struct SyncLane {
        #[serde(default)]
        pub last_successful_sync: Option<String>,
    }

    #[derive(Deserialize)]
    pub struct AccountBalance {
        #[serde(default)]
        pub total: Option<Money>,
    }

    #[derive(Deserialize)]
    pub struct Money {
        #[serde(default, deserialize_with = "lenient_f64")]
        pub amount: Option<f64>,
        #[serde(default)]
        pub currency: Option<String>,
    }

    #[derive(Deserialize)]
    pub struct Currency {
        #[serde(default)]
        pub code: Option<String>,
    }

    #[derive(Deserialize)]
    pub struct Balance {
        #[serde(default)]
        pub currency: Option<Currency>,
        #[serde(default, deserialize_with = "lenient_f64")]
        pub cash: Option<f64>,
    }

    #[derive(Deserialize)]
    pub struct Positions {
        #[serde(default)]
        pub results: Vec<Position>,
        #[serde(default)]
        pub data_freshness: Option<Freshness>,
    }

    #[derive(Deserialize)]
    pub struct Freshness {
        #[serde(default)]
        pub as_of: Option<String>,
    }

    #[derive(Deserialize)]
    pub struct Position {
        pub instrument: Instrument,
        #[serde(default, deserialize_with = "lenient_f64")]
        pub units: Option<f64>,
        #[serde(default, deserialize_with = "lenient_f64")]
        pub price: Option<f64>,
        #[serde(default, deserialize_with = "lenient_f64")]
        pub cost_basis: Option<f64>,
        #[serde(default)]
        pub currency: Option<String>,
        #[serde(default)]
        pub cash_equivalent: Option<bool>,
    }

    #[derive(Deserialize)]
    pub struct Instrument {
        #[serde(default)]
        pub kind: Option<String>,
        pub symbol: String,
        #[serde(default)]
        pub description: Option<String>,
        #[serde(default)]
        pub currency: Option<String>,
    }

    #[derive(Deserialize)]
    pub struct ActivityPage {
        #[serde(default)]
        pub data: Vec<Activity>,
        #[serde(default)]
        pub pagination: Option<Pagination>,
    }

    #[derive(Deserialize)]
    pub struct Pagination {
        #[serde(default)]
        pub total: Option<u64>,
    }

    #[derive(Deserialize)]
    pub struct Activity {
        pub id: String,
        #[serde(default)]
        pub symbol: Option<Symbol>,
        #[serde(default, deserialize_with = "lenient_f64")]
        pub price: Option<f64>,
        #[serde(default, deserialize_with = "lenient_f64")]
        pub units: Option<f64>,
        #[serde(default, deserialize_with = "lenient_f64")]
        pub amount: Option<f64>,
        #[serde(default)]
        pub currency: Option<Currency>,
        #[serde(rename = "type", default)]
        pub kind: Option<String>,
        #[serde(default)]
        pub description: Option<String>,
        #[serde(default)]
        pub trade_date: Option<String>,
        #[serde(default)]
        pub settlement_date: Option<String>,
        #[serde(default, deserialize_with = "lenient_f64")]
        pub fee: Option<f64>,
    }

    #[derive(Deserialize)]
    pub struct Symbol {
        #[serde(default)]
        pub symbol: Option<String>,
    }

    fn lenient_f64<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<f64>, D::Error> {
        Ok(
            match Option::<serde_json::Value>::deserialize(deserializer)? {
                Some(serde_json::Value::Number(number)) => number.as_f64(),
                Some(serde_json::Value::String(text)) => text.trim().parse().ok(),
                _ => None,
            }
            .filter(|number: &f64| number.is_finite()),
        )
    }
}

/// `Signature` header for a signed SnapTrade request.
///
/// SnapTrade signs the compact, key-sorted JSON object
/// `{"content":…,"path":…,"query":…}` with HMAC-SHA256 under the consumer key
/// and sends it base64-encoded. This client only issues bodiless `GET`s, so
/// `content` is always `null`.
pub fn signature(consumer_key: &str, path: &str, query: &str) -> String {
    let payload = format!(
        r#"{{"content":null,"path":{},"query":{}}}"#,
        serde_json::Value::from(path),
        serde_json::Value::from(query),
    );
    let mut mac = Hmac::<Sha256>::new_from_slice(consumer_key.as_bytes())
        .expect("HMAC-SHA256 accepts keys of any length");
    mac.update(payload.as_bytes());
    base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes())
}

/// A SnapTrade Personal API key: a client ID plus the consumer key that signs
/// requests. `Debug` never prints the consumer key.
#[derive(Clone)]
pub struct Credentials {
    pub client_id: String,
    pub consumer_key: String,
}

impl std::fmt::Debug for Credentials {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Credentials")
            .field("client_id", &self.client_id)
            .field("consumer_key", &"<redacted>")
            .finish()
    }
}

pub const PRODUCTION_BASE_URL: &str = "https://api.snaptrade.com";

/// Read-only SnapTrade client. Every request is a signed `GET` against an
/// account-data path; the type has no method that could reach a trading or
/// money-movement endpoint.
pub struct Client {
    credentials: Credentials,
    base_url: String,
    http: reqwest::blocking::Client,
    now: fn() -> i64,
    page_limit: u32,
}

impl Client {
    pub fn new(credentials: Credentials) -> Result<Self, FinanceError> {
        Self::with_base_url(credentials, PRODUCTION_BASE_URL, unix_now)
    }

    pub fn with_base_url(
        credentials: Credentials,
        base_url: &str,
        now: fn() -> i64,
    ) -> Result<Self, FinanceError> {
        let http = crate::http::client(SOURCE)?;
        Ok(Self {
            credentials,
            base_url: base_url.trim_end_matches('/').to_string(),
            http,
            now,
            page_limit: 1000,
        })
    }

    /// Page size for activity reads (SnapTrade allows up to 1000).
    pub fn page_limit(mut self, limit: u32) -> Self {
        self.page_limit = limit.clamp(1, 1000);
        self
    }

    pub fn accounts(&self) -> Result<Vec<Account>, FinanceError> {
        parse_accounts(&self.get("/accounts", &[])?)
    }

    pub fn balances(&self, account_id: &str) -> Result<Vec<CashBalance>, FinanceError> {
        let path = account_path(account_id, "balances")?;
        parse_balances(&self.get(&path, &[])?)
    }

    pub fn positions(
        &self,
        account_id: &str,
    ) -> Result<(Vec<Holding>, Option<String>), FinanceError> {
        let path = account_path(account_id, "positions/all")?;
        parse_positions(account_id, &self.get(&path, &[])?)
    }

    /// Every activity since `start_date` (`YYYY-MM-DD`), or the whole history
    /// SnapTrade holds when `None`, following pagination to the end.
    pub fn activities(
        &self,
        account_id: &str,
        start_date: Option<&str>,
    ) -> Result<Vec<Activity>, FinanceError> {
        let path = account_path(account_id, "activities")?;
        let mut activities = Vec::new();
        let mut offset: u64 = 0;
        loop {
            let mut params = Vec::new();
            if let Some(start) = start_date {
                params.push(("startDate", start.to_string()));
            }
            params.push(("offset", offset.to_string()));
            params.push(("limit", self.page_limit.to_string()));
            let page = parse_activities(account_id, &self.get(&path, &params)?)?;
            let fetched = page.activities.len() as u64;
            activities.extend(page.activities);
            offset += fetched;
            let exhausted = page
                .total
                .map_or(fetched < u64::from(self.page_limit), |total| {
                    offset >= total
                });
            if fetched == 0 || exhausted {
                return Ok(activities);
            }
        }
    }

    /// Signed `GET`. Operation parameters come first, then `clientId` and
    /// `timestamp`, matching SnapTrade's SDKs; the exact query string sent is
    /// the one signed.
    fn get(&self, path: &str, params: &[(&str, String)]) -> Result<String, FinanceError> {
        let timestamp = (self.now)().to_string();
        let mut pairs: Vec<(&str, &str)> = params
            .iter()
            .map(|(key, value)| (*key, value.as_str()))
            .collect();
        pairs.push(("clientId", &self.credentials.client_id));
        pairs.push(("timestamp", &timestamp));
        let query = crate::http::query(&pairs);
        let signed = signature(&self.credentials.consumer_key, path, &query);
        let response = self
            .http
            .get(format!("{}{path}?{query}", self.base_url))
            .header("Signature", signed)
            .header("Accept", "application/json")
            .send()
            .map_err(|error| crate::http::transport(SOURCE, &error))?;
        let status = response.status().as_u16();
        let retry_after_seconds = response
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.trim().parse().ok());
        let body = response
            .text()
            .map_err(|error| crate::http::transport(SOURCE, &error))?;
        match status {
            200..=299 => Ok(body),
            401 | 403 => Err(FinanceError::Auth(format!(
                "SnapTrade refused the Personal API key (HTTP {status}); reconnect with `heiwa connect snaptrade`"
            ))),
            429 => Err(FinanceError::RateLimited { source_name: SOURCE.into(), retry_after_seconds }),
            503 => Err(FinanceError::Busy { source_name: SOURCE.into() }),
            _ => Err(FinanceError::Http { source_name: SOURCE.into(), status, detail: error_detail(&body) }),
        }
    }
}

/// `/accounts/{id}/{leaf}` for an id that cannot leave the read path. Account
/// ids are UUIDs; anything else is refused before a request is made.
fn account_path(account_id: &str, leaf: &str) -> Result<String, FinanceError> {
    let safe = !account_id.is_empty()
        && account_id.len() <= 64
        && account_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-');
    if !safe {
        return Err(FinanceError::Decode {
            source_name: SOURCE.into(),
            detail: "account id is not a SnapTrade account id".into(),
        });
    }
    Ok(format!("/accounts/{account_id}/{leaf}"))
}

/// The `detail` of a SnapTrade error body, or a bounded prefix of it.
fn error_detail(body: &str) -> String {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| {
            value
                .get("detail")
                .and_then(|detail| detail.as_str().map(str::to_string))
        })
        .unwrap_or_else(|| body.chars().take(200).collect())
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::AccountKind;
    use crate::test_support::{header, request_line, stub};

    fn test_credentials() -> Credentials {
        Credentials {
            client_id: "TEST-CLIENT".into(),
            consumer_key: "test-consumer-key".into(),
        }
    }

    fn fixed_now() -> i64 {
        1_700_000_000
    }

    #[test]
    fn get_requests_carry_client_id_then_timestamp_and_a_valid_signature() {
        let server = stub(vec![(200, "", "[]".into())]);
        let client = Client::with_base_url(test_credentials(), &server.url, fixed_now).unwrap();
        assert!(client.accounts().unwrap().is_empty());
        server.thread.join().unwrap();
        let requests = server.requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        let raw = &requests[0];
        assert_eq!(
            request_line(raw),
            "GET /accounts?clientId=TEST-CLIENT&timestamp=1700000000 HTTP/1.1"
        );
        assert_eq!(
            header(raw, "signature"),
            Some("fxoKfD28V3siYq2CYON1/xtA/t8odndPWF95ZVQ1C9M=")
        );
        assert!(
            !raw.contains("test-consumer-key"),
            "the consumer key must never be sent"
        );
        assert!(
            !raw.contains("userSecret"),
            "Personal keys carry no user secret"
        );
    }

    #[test]
    fn activities_follow_pagination_with_operation_params_signed_first() {
        let page = |ids: &[&str], offset: u64| {
            let rows: Vec<String> = ids
                .iter()
                .map(|id| format!(r#"{{"id":"{id}","type":"CONTRIBUTION","amount":250,"settlement_date":"2026-08-01"}}"#))
                .collect();
            format!(
                r#"{{"data":[{}],"pagination":{{"offset":{offset},"limit":2,"total":3}}}}"#,
                rows.join(",")
            )
        };
        let server = stub(vec![
            (200, "", page(&["a1", "a2"], 0)),
            (200, "", page(&["a3"], 2)),
        ]);
        let client = Client::with_base_url(test_credentials(), &server.url, fixed_now)
            .unwrap()
            .page_limit(2);
        let activities = client
            .activities("0a1b2c3d-0000-4000-8000-000000000001", Some("2026-01-01"))
            .unwrap();
        server.thread.join().unwrap();
        assert_eq!(
            activities
                .iter()
                .map(|activity| activity.id.as_str())
                .collect::<Vec<_>>(),
            ["a1", "a2", "a3"]
        );
        let requests = server.requests.lock().unwrap();
        let path = "/accounts/0a1b2c3d-0000-4000-8000-000000000001/activities";
        let second_query =
            "startDate=2026-01-01&offset=2&limit=2&clientId=TEST-CLIENT&timestamp=1700000000";
        assert_eq!(
            request_line(&requests[1]),
            format!("GET {path}?{second_query} HTTP/1.1")
        );
        assert_eq!(
            header(&requests[1], "signature"),
            Some(signature("test-consumer-key", path, second_query).as_str())
        );
    }

    #[test]
    fn http_failures_become_actionable_errors_that_never_carry_the_key() {
        let server = stub(vec![
            (401, "", r#"{"detail":"Invalid signature"}"#.into()),
            (
                429,
                "Retry-After: 7\r\n",
                r#"{"detail":"slow down"}"#.into(),
            ),
            (503, "", r#"{"detail":"sync lock held"}"#.into()),
            (500, "", r#"{"detail":"boom","status_code":500}"#.into()),
        ]);
        let client = Client::with_base_url(test_credentials(), &server.url, fixed_now).unwrap();
        let errors: Vec<FinanceError> = (0..4).map(|_| client.accounts().unwrap_err()).collect();
        server.thread.join().unwrap();
        assert!(
            matches!(errors[0], FinanceError::Auth(_)),
            "{:?}",
            errors[0]
        );
        assert!(matches!(
            errors[1],
            FinanceError::RateLimited {
                retry_after_seconds: Some(7),
                ..
            }
        ));
        assert!(matches!(errors[2], FinanceError::Busy { .. }));
        assert!(
            matches!(&errors[3], FinanceError::Http { status: 500, detail, .. } if detail == "boom"),
            "{:?}",
            errors[3]
        );
        for error in &errors {
            assert!(!error.to_string().contains("test-consumer-key"));
            assert!(!format!("{error:?}").contains("test-consumer-key"));
        }
    }

    #[test]
    fn account_ids_that_could_leave_the_read_path_are_refused_before_any_request() {
        // Port 9 (discard) is never contacted: validation happens first.
        let client =
            Client::with_base_url(test_credentials(), "http://127.0.0.1:9", fixed_now).unwrap();
        for id in ["../trade/place", "a/b", "a?b", "", "a b"] {
            assert!(
                matches!(client.positions(id), Err(FinanceError::Decode { .. })),
                "{id:?} must be refused"
            );
        }
    }

    #[test]
    fn credentials_debug_output_redacts_the_consumer_key() {
        let rendered = format!("{:?}", test_credentials());
        assert!(rendered.contains("TEST-CLIENT"));
        assert!(!rendered.contains("test-consumer-key"));
    }

    /// Product invariant: Heiwa never trades or moves money. The brokerage
    /// client must stay GET-only against read paths; adding a write verb or
    /// a trading path to this module fails here, in review, not in production.
    #[test]
    fn the_brokerage_client_has_no_write_or_trading_path() {
        let source = include_str!("snaptrade.rs");
        let production = &source[..source.find("#[cfg(test)]").unwrap()];
        for forbidden in [
            ".post(",
            ".put(",
            ".patch(",
            ".delete(",
            "Method::POST",
            "Method::PUT",
            "Method::DELETE",
            "\"/trade",
            "/orders",
            "/trading",
        ] {
            assert!(
                !production.contains(forbidden),
                "read-only client contains {forbidden:?}"
            );
        }
    }

    const ACCOUNTS: &str = r#"[{
        "id": "0a1b2c3d-0000-4000-8000-000000000001",
        "brokerage_authorization": "0a1b2c3d-0000-4000-8000-0000000000aa",
        "name": "Wealthsimple TFSA",
        "number": "HQ1234567",
        "institution_name": "Wealthsimple",
        "created_date": "2026-07-01T00:00:00Z",
        "sync_status": {
            "holdings": {"initial_sync_completed": true, "last_successful_sync": "2026-09-24T12:00:00Z"},
            "transactions": {"initial_sync_completed": true, "last_successful_sync": "2026-09-24", "first_transaction_date": "2026-07-02"}
        },
        "balance": {"total": {"amount": 1523.45, "currency": "CAD"}},
        "raw_type": "TFSA",
        "status": "open",
        "is_paper": false,
        "meta": {"type": "TFSA"},
        "future_field": {"ignored": true}
    }, {
        "id": "0a1b2c3d-0000-4000-8000-000000000002",
        "brokerage_authorization": "0a1b2c3d-0000-4000-8000-0000000000aa",
        "name": null,
        "number": "12",
        "institution_name": "Wealthsimple",
        "created_date": "2026-07-01T00:00:00Z",
        "sync_status": {},
        "balance": {"total": null},
        "raw_type": null,
        "is_paper": false
    }]"#;

    #[test]
    fn parse_accounts_classifies_and_drops_the_full_account_number() {
        let accounts = parse_accounts(ACCOUNTS).unwrap();
        assert_eq!(accounts.len(), 2);
        let tfsa = &accounts[0];
        assert_eq!(tfsa.source, "snaptrade");
        assert_eq!(tfsa.institution, "Wealthsimple");
        assert_eq!(tfsa.name, "Wealthsimple TFSA");
        assert_eq!(tfsa.kind, AccountKind::Tfsa);
        assert_eq!(tfsa.number_hint.as_deref(), Some("4567"));
        assert_eq!(tfsa.total_value, Some(1523.45));
        assert_eq!(tfsa.currency.as_deref(), Some("CAD"));
        assert_eq!(
            tfsa.holdings_synced_at.as_deref(),
            Some("2026-09-24T12:00:00Z")
        );
        assert_eq!(tfsa.transactions_synced_at.as_deref(), Some("2026-09-24"));
        assert!(!serde_json::to_string(&accounts)
            .unwrap()
            .contains("HQ1234567"));

        let bare = &accounts[1];
        assert_eq!(bare.name, "Wealthsimple account");
        assert_eq!(bare.kind, AccountKind::Other);
        assert_eq!(bare.number_hint, None);
        assert_eq!(bare.total_value, None);
    }

    #[test]
    fn parse_accounts_rejects_a_non_list_body() {
        assert!(parse_accounts(r#"{"detail":"nope"}"#).is_err());
    }

    #[test]
    fn parse_balances_keeps_one_cash_row_per_currency() {
        let cash = parse_balances(
            r#"[{"currency":{"id":"x","code":"CAD","name":"Canadian Dollar"},"cash":12.34,"buying_power":12.34},
                {"currency":{"code":"USD"},"cash":null},
                {"cash":5.0}]"#,
        )
        .unwrap();
        assert_eq!(
            cash,
            vec![
                CashBalance {
                    currency: "CAD".into(),
                    amount: 12.34
                },
                CashBalance {
                    currency: "USD".into(),
                    amount: 0.0
                },
            ]
        );
    }

    #[test]
    fn parse_positions_accepts_decimal_strings_and_skips_unitless_rows() {
        let (holdings, as_of) = parse_positions(
            "acct-1",
            r#"{"results":[
                {"instrument":{"kind":"etf","id":"i1","symbol":"XEQT.TO","raw_symbol":"XEQT","description":"iShares Core Equity ETF Portfolio","currency":"CAD","exchange":"TSX"},
                 "units":"12.5","price":"34.10","cost_basis":"30.00","currency":"CAD"},
                {"instrument":{"kind":"crypto","id":"i2","symbol":"BTC","raw_symbol":"BTC"},
                 "units":0.001,"price":85000,"cost_basis":null,"currency":"CAD","cash_equivalent":false},
                {"instrument":{"kind":"stock","id":"i3","symbol":"SHOP.TO","raw_symbol":"SHOP"},
                 "units":null,"price":"150.00"}
             ],
             "data_freshness":{"as_of":"2026-09-24 14:30:00+00:00"}}"#,
        )
        .unwrap();
        assert_eq!(as_of.as_deref(), Some("2026-09-24 14:30:00+00:00"));
        assert_eq!(holdings.len(), 2);
        let etf = &holdings[0];
        assert_eq!(etf.account_id, "acct-1");
        assert_eq!(etf.symbol, "XEQT.TO");
        assert_eq!(etf.kind, "etf");
        assert_eq!(etf.units, 12.5);
        assert_eq!(etf.price, Some(34.10));
        assert_eq!(etf.average_cost, Some(30.0));
        assert_eq!(etf.currency.as_deref(), Some("CAD"));
        let btc = &holdings[1];
        assert_eq!(btc.symbol, "BTC");
        assert_eq!(btc.price, Some(85000.0));
        assert_eq!(btc.average_cost, None);
    }

    #[test]
    fn parse_activities_normalizes_types_dates_and_symbols() {
        let page = parse_activities(
            "acct-1",
            r#"{"data":[
                {"id":"a1","symbol":{"id":"s1","symbol":"XEQT.TO","raw_symbol":"XEQT","currency":{"code":"CAD"}},
                 "price":33.5,"units":7,"amount":-234.5,"currency":{"code":"CAD"},"type":"BUY",
                 "description":"Bought 7 XEQT","trade_date":"2026-08-03 14:05:00+00:00","settlement_date":"2026-08-05 00:00:00+00:00","fee":0},
                {"id":"a2","symbol":null,"price":0,"units":0,"amount":250,"currency":{"code":"CAD"},"type":"CONTRIBUTION",
                 "description":"Deposit","trade_date":null,"settlement_date":"2026-08-01T00:00:00Z","fee":0},
                {"id":"a3","type":"SUBSTITUTE_DIVIDEND","amount":1.25,"settlement_date":"2026-09-02"},
                {"id":"a4","type":"SOMETHING_NEW","settlement_date":"2026-09-03"}
             ],
             "pagination":{"offset":0,"limit":1000,"total":4}}"#,
        )
        .unwrap();
        assert_eq!(page.total, Some(4));
        let [buy, deposit, dividend, unknown] = page.activities.as_slice() else {
            panic!("expected four activities, got {}", page.activities.len());
        };
        assert_eq!(buy.kind, ActivityKind::Buy);
        assert_eq!(buy.symbol.as_deref(), Some("XEQT.TO"));
        assert_eq!(buy.units, Some(7.0));
        assert_eq!(buy.amount, Some(-234.5));
        assert_eq!(buy.trade_date.as_deref(), Some("2026-08-03"));
        assert_eq!(buy.settlement_date.as_deref(), Some("2026-08-05"));
        assert_eq!(deposit.kind, ActivityKind::Contribution);
        assert_eq!(deposit.symbol, None);
        assert_eq!(
            deposit.trade_date.as_deref(),
            Some("2026-08-01"),
            "falls back to settlement"
        );
        assert_eq!(dividend.kind, ActivityKind::Dividend);
        assert_eq!(unknown.kind, ActivityKind::Other);
        assert_eq!(unknown.raw_type, "SOMETHING_NEW");
    }

    #[test]
    fn activity_kind_maps_the_documented_snaptrade_types() {
        for (raw, kind) in [
            ("BUY", ActivityKind::Buy),
            ("SELL", ActivityKind::Sell),
            ("DIVIDEND", ActivityKind::Dividend),
            ("STOCK_DIVIDEND", ActivityKind::Dividend),
            ("REI", ActivityKind::Reinvest),
            ("CONTRIBUTION", ActivityKind::Contribution),
            ("WITHDRAWAL", ActivityKind::Withdrawal),
            ("INTEREST", ActivityKind::Interest),
            ("FEE", ActivityKind::Fee),
            ("TAX", ActivityKind::Tax),
            ("TRANSFER", ActivityKind::Transfer),
            ("EXTERNAL_ASSET_TRANSFER_IN", ActivityKind::Transfer),
            ("SPLIT", ActivityKind::Split),
            ("OPTIONEXPIRATION", ActivityKind::Other),
        ] {
            assert_eq!(activity_kind(raw), kind, "{raw}");
        }
    }

    #[test]
    fn signature_matches_the_reference_sdk_algorithm() {
        assert_eq!(
            signature(
                "test-consumer-key",
                "/accounts",
                "clientId=TEST-CLIENT&timestamp=1700000000"
            ),
            "fxoKfD28V3siYq2CYON1/xtA/t8odndPWF95ZVQ1C9M="
        );
        assert_eq!(
            signature(
                "test-consumer-key",
                "/accounts/5f1c2d3e-0000-4000-8000-000000000001/activities",
                "clientId=TEST-CLIENT&timestamp=1700000000&startDate=2026-01-01&endDate=2026-09-25&offset=0&limit=1000"
            ),
            "FURHxCS3UrJt38FEt4CTY9/JTMxrKMvzUU6pLjhIo+A="
        );
    }
}
