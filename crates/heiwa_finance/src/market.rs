//! Public market data: daily FX from the Bank of Canada and daily bars from
//! Alpha Vantage (bring-your-own key).

use crate::FinanceError;
use serde::{Deserialize, Serialize};

/// One daily bar. Only `close` is required; the rest are kept when the
/// source reports them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bar {
    pub date: String,
    pub open: Option<f64>,
    pub high: Option<f64>,
    pub low: Option<f64>,
    pub close: f64,
    pub volume: Option<f64>,
}

/// Units of the quote currency per one unit of the base currency on `date`
/// (for `USD/CAD`, Canadian dollars per US dollar).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FxRate {
    pub date: String,
    pub rate: f64,
}

pub mod bank_of_canada {
    use super::*;

    pub const BASE_URL: &str = "https://www.bankofcanada.ca/valet";
    const SOURCE: &str = "Bank of Canada";

    /// Valet series for `base`→CAD. The Bank publishes daily rates against
    /// the Canadian dollar only.
    pub fn series(base: &str) -> Option<String> {
        let code = base.trim().to_ascii_uppercase();
        (code.len() == 3 && code.bytes().all(|byte| byte.is_ascii_uppercase()) && code != "CAD")
            .then(|| format!("FX{code}CAD"))
    }

    pub fn parse_observations(series: &str, body: &str) -> Result<Vec<FxRate>, FinanceError> {
        #[derive(Deserialize)]
        struct Response {
            observations: Vec<serde_json::Map<String, serde_json::Value>>,
        }
        let response: Response =
            serde_json::from_str(body).map_err(|error| FinanceError::Decode {
                source_name: SOURCE.into(),
                detail: error.to_string(),
            })?;
        let mut rates: Vec<FxRate> = response
            .observations
            .iter()
            .filter_map(|row| {
                let date = row.get("d")?.as_str()?;
                let rate: f64 = row.get(series)?.get("v")?.as_str()?.trim().parse().ok()?;
                (crate::http::is_iso_date(date) && rate.is_finite() && rate > 0.0).then(|| FxRate {
                    date: date.to_string(),
                    rate,
                })
            })
            .collect();
        rates.sort_by(|a, b| a.date.cmp(&b.date));
        Ok(rates)
    }

    pub struct Client {
        base_url: String,
        http: reqwest::blocking::Client,
    }

    impl Client {
        pub fn new() -> Result<Self, FinanceError> {
            Self::with_base_url(BASE_URL)
        }

        pub fn with_base_url(base_url: &str) -> Result<Self, FinanceError> {
            Ok(Self {
                base_url: base_url.trim_end_matches('/').to_string(),
                http: crate::http::client(SOURCE)?,
            })
        }

        /// Daily observations since `start_date`, oldest first.
        pub fn observations(
            &self,
            series: &str,
            start_date: &str,
        ) -> Result<Vec<FxRate>, FinanceError> {
            let valid_series = series.len() == 8
                && series.starts_with("FX")
                && series.ends_with("CAD")
                && series.bytes().all(|byte| byte.is_ascii_uppercase());
            if !valid_series || !crate::http::is_iso_date(start_date) {
                return Err(FinanceError::Decode {
                    source_name: SOURCE.into(),
                    detail: "not a Valet FX series and start date".into(),
                });
            }
            let url = format!(
                "{}/observations/{series}/json?{}",
                self.base_url,
                crate::http::query(&[("start_date", start_date)])
            );
            let (status, body) = crate::http::read(SOURCE, self.http.get(url).send())?;
            if !(200..300).contains(&status) {
                return Err(FinanceError::Http {
                    source_name: SOURCE.into(),
                    status,
                    detail: body.chars().take(200).collect(),
                });
            }
            parse_observations(series, &body)
        }
    }
}

pub mod alpha_vantage {
    use super::*;

    pub const BASE_URL: &str = "https://www.alphavantage.co/query";
    const SOURCE: &str = "Alpha Vantage";

    /// Alpha Vantage's ticker for a Yahoo-style symbol: TSX `.TO` becomes
    /// `.TRT`, TSX Venture `.V` becomes `.TRV`, US listings pass through.
    /// Venues Alpha Vantage does not cover return `None`.
    pub fn provider_symbol(symbol: &str) -> Option<String> {
        let symbol = symbol.trim().to_ascii_uppercase();
        let well_formed = !symbol.is_empty()
            && symbol.len() <= 24
            && !symbol.starts_with('.')
            && symbol
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'.' || byte == b'-');
        if !well_formed {
            return None;
        }
        match symbol.rsplit_once('.') {
            Some((base, "TO")) => Some(format!("{base}.TRT")),
            Some((base, "V")) => Some(format!("{base}.TRV")),
            Some((_, "NE" | "NEO" | "CN")) => None,
            _ => Some(symbol),
        }
    }

    /// `TIME_SERIES_DAILY`, oldest first. Alpha Vantage reports errors and
    /// quota exhaustion as HTTP 200 envelopes; those become errors here.
    pub fn parse_daily(body: &str) -> Result<Vec<Bar>, FinanceError> {
        let value: serde_json::Value = serde_json::from_str(body).map_err(decode)?;
        let object = value
            .as_object()
            .ok_or_else(|| decode("response is not an object"))?;
        if let Some(message) = object.get("Error Message").and_then(|value| value.as_str()) {
            return Err(FinanceError::Http {
                source_name: SOURCE.into(),
                status: 200,
                detail: message.chars().take(200).collect(),
            });
        }
        for key in ["Information", "Note"] {
            if let Some(message) = object.get(key).and_then(|value| value.as_str()) {
                let lower = message.to_ascii_lowercase();
                return Err(
                    if ["rate limit", "per day", "per minute", "frequency"]
                        .iter()
                        .any(|needle| lower.contains(needle))
                    {
                        FinanceError::RateLimited {
                            source_name: SOURCE.into(),
                            retry_after_seconds: None,
                        }
                    } else {
                        FinanceError::Auth(format!(
                            "Alpha Vantage: {}",
                            message.chars().take(200).collect::<String>()
                        ))
                    },
                );
            }
        }
        let series = object
            .get("Time Series (Daily)")
            .and_then(|series| series.as_object())
            .ok_or_else(|| decode("no daily series in the response"))?;
        let field = |row: &serde_json::Value, key: &str| -> Option<f64> {
            row.get(key)?
                .as_str()?
                .trim()
                .parse()
                .ok()
                .filter(|number: &f64| number.is_finite())
        };
        let mut bars: Vec<Bar> = series
            .iter()
            .filter(|(date, _)| crate::http::is_iso_date(date))
            .filter_map(|(date, row)| {
                Some(Bar {
                    date: date.clone(),
                    close: field(row, "4. close")?,
                    open: field(row, "1. open"),
                    high: field(row, "2. high"),
                    low: field(row, "3. low"),
                    volume: field(row, "5. volume"),
                })
            })
            .collect();
        bars.sort_by(|a, b| a.date.cmp(&b.date));
        Ok(bars)
    }

    fn decode(detail: impl std::fmt::Display) -> FinanceError {
        FinanceError::Decode {
            source_name: SOURCE.into(),
            detail: detail.to_string(),
        }
    }

    pub struct Client {
        api_key: String,
        base_url: String,
        http: reqwest::blocking::Client,
    }

    impl Client {
        pub fn new(api_key: String) -> Result<Self, FinanceError> {
            Self::with_base_url(api_key, BASE_URL)
        }

        pub fn with_base_url(api_key: String, base_url: &str) -> Result<Self, FinanceError> {
            Ok(Self {
                api_key,
                base_url: base_url.trim_end_matches('/').to_string(),
                http: crate::http::client(SOURCE)?,
            })
        }

        /// The latest ~100 daily bars (the free tier's compact window).
        pub fn daily(&self, symbol: &str) -> Result<Vec<Bar>, FinanceError> {
            let mapped = provider_symbol(symbol)
                .ok_or_else(|| decode(format!("Alpha Vantage does not cover {symbol}")))?;
            let query = crate::http::query(&[
                ("function", "TIME_SERIES_DAILY"),
                ("symbol", &mapped),
                ("outputsize", "compact"),
                ("apikey", &self.api_key),
            ]);
            let (status, body) = crate::http::read(
                SOURCE,
                self.http.get(format!("{}?{query}", self.base_url)).send(),
            )?;
            if !(200..300).contains(&status) {
                return Err(FinanceError::Http {
                    source_name: SOURCE.into(),
                    status,
                    detail: body.chars().take(200).collect(),
                });
            }
            parse_daily(&body)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{request_line, stub};

    const VALET: &str = r#"{
        "terms": {"url": "https://www.bankofcanada.ca/terms/"},
        "seriesDetail": {"FXUSDCAD": {"label": "USD/CAD"}},
        "observations": [
            {"d": "2026-09-25", "FXUSDCAD": {"v": "1.4145"}},
            {"d": "2026-09-23", "FXUSDCAD": {"v": "1.4096"}},
            {"d": "2026-09-24", "FXUSDCAD": {"v": ""}},
            {"d": "2026-09-22"}
        ]
    }"#;

    #[test]
    fn valet_series_exists_only_for_rates_against_the_canadian_dollar() {
        assert_eq!(bank_of_canada::series("USD").as_deref(), Some("FXUSDCAD"));
        assert_eq!(bank_of_canada::series("eur").as_deref(), Some("FXEURCAD"));
        assert_eq!(bank_of_canada::series("CAD"), None);
        assert_eq!(bank_of_canada::series("US"), None);
        assert_eq!(bank_of_canada::series("U$D"), None);
    }

    #[test]
    fn valet_observations_parse_oldest_first_and_skip_blank_values() {
        let rates = bank_of_canada::parse_observations("FXUSDCAD", VALET).unwrap();
        assert_eq!(
            rates,
            vec![
                FxRate {
                    date: "2026-09-23".into(),
                    rate: 1.4096
                },
                FxRate {
                    date: "2026-09-25".into(),
                    rate: 1.4145
                },
            ]
        );
    }

    #[test]
    fn valet_client_requests_the_series_from_a_start_date() {
        let server = stub(vec![(200, "", VALET.into())]);
        let client = bank_of_canada::Client::with_base_url(&server.url).unwrap();
        let rates = client.observations("FXUSDCAD", "2026-09-01").unwrap();
        server.thread.join().unwrap();
        assert_eq!(rates.len(), 2);
        let requests = server.requests.lock().unwrap();
        assert_eq!(
            request_line(&requests[0]),
            "GET /observations/FXUSDCAD/json?start_date=2026-09-01 HTTP/1.1"
        );
    }

    #[test]
    fn provider_symbol_maps_canadian_venues_and_refuses_unknown_ones() {
        use alpha_vantage::provider_symbol;
        assert_eq!(provider_symbol("XEQT.TO").as_deref(), Some("XEQT.TRT"));
        assert_eq!(provider_symbol("abc.v").as_deref(), Some("ABC.TRV"));
        assert_eq!(provider_symbol("AAPL").as_deref(), Some("AAPL"));
        assert_eq!(provider_symbol("BRK.B").as_deref(), Some("BRK.B"));
        assert_eq!(provider_symbol("ZSP.NE"), None);
        assert_eq!(provider_symbol("../x"), None);
        assert_eq!(provider_symbol(""), None);
    }

    #[test]
    fn daily_series_parses_oldest_first_with_the_close_required() {
        let bars = alpha_vantage::parse_daily(
            r#"{"Meta Data": {"2. Symbol": "SHOP.TRT"},
                "Time Series (Daily)": {
                    "2026-09-25": {"1. open": "205.5100", "2. high": "205.5100", "3. low": "199.4000", "4. close": "201.2000", "5. volume": "1467319"},
                    "2026-09-24": {"1. open": "197.8500", "4. close": "205.3000"},
                    "2026-09-23": {"1. open": "206.4200"}
                }}"#,
        )
        .unwrap();
        assert_eq!(bars.len(), 2);
        assert_eq!(bars[0].date, "2026-09-24");
        assert_eq!(bars[0].close, 205.3);
        assert_eq!(bars[0].high, None);
        assert_eq!(bars[1].date, "2026-09-25");
        assert_eq!(bars[1].close, 201.2);
        assert_eq!(bars[1].volume, Some(1_467_319.0));
    }

    #[test]
    fn daily_series_error_envelopes_are_errors_not_empty_data() {
        let rate_limited = alpha_vantage::parse_daily(
            r#"{"Information": "Our standard API rate limit is 25 requests per day."}"#,
        );
        assert!(matches!(
            rate_limited,
            Err(FinanceError::RateLimited { .. })
        ));
        let note = alpha_vantage::parse_daily(
            r#"{"Note": "Thank you for using Alpha Vantage! Our standard API call frequency is 5 calls per minute."}"#,
        );
        assert!(matches!(note, Err(FinanceError::RateLimited { .. })));
        let demo = alpha_vantage::parse_daily(
            r#"{"Information": "The **demo** API key is for demo purposes only."}"#,
        );
        assert!(matches!(demo, Err(FinanceError::Auth(_))));
        let unknown = alpha_vantage::parse_daily(
            r#"{"Error Message": "Invalid API call. Please retry or visit the documentation."}"#,
        );
        assert!(matches!(unknown, Err(FinanceError::Http { .. })));
        assert!(alpha_vantage::parse_daily("[]").is_err());
    }

    #[test]
    fn daily_client_sends_the_mapped_symbol_and_keeps_the_key_out_of_errors() {
        let server = stub(vec![(
            200,
            "",
            r#"{"Error Message": "Invalid API call."}"#.into(),
        )]);
        let client =
            alpha_vantage::Client::with_base_url("test-market-key".into(), &server.url).unwrap();
        let error = client.daily("XEQT.TO").unwrap_err();
        server.thread.join().unwrap();
        let requests = server.requests.lock().unwrap();
        assert_eq!(
            request_line(&requests[0]),
            "GET /?function=TIME_SERIES_DAILY&symbol=XEQT.TRT&outputsize=compact&apikey=test-market-key HTTP/1.1"
        );
        assert!(!error.to_string().contains("test-market-key"));
        assert!(!format!("{error:?}").contains("test-market-key"));
    }

    #[test]
    fn daily_client_refuses_symbols_it_cannot_map_without_a_request() {
        let client =
            alpha_vantage::Client::with_base_url("test-market-key".into(), "http://127.0.0.1:9")
                .unwrap();
        assert!(matches!(
            client.daily("ZSP.NE"),
            Err(FinanceError::Decode { .. })
        ));
    }
}
