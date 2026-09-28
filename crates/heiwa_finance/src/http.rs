//! The one HTTP client shape every finance source uses.

use crate::FinanceError;
use std::time::Duration;

/// Bounded, redirect-free blocking client. Finance reads run on blocking
/// threads (the CLI, or `spawn_blocking` inside the runtime).
pub(crate) fn client(source_name: &str) -> Result<reqwest::blocking::Client, FinanceError> {
    reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(45))
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(concat!("heiwa-finance/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|error| transport(source_name, &error))
}

/// reqwest errors can embed the request URL, and some sources (Alpha
/// Vantage) put the API key in the query string; drop the URL so neither
/// reaches a log or a UI.
pub(crate) fn transport(source_name: &str, error: &reqwest::Error) -> FinanceError {
    let detail = if error.is_timeout() {
        "timed out".to_string()
    } else if error.is_connect() {
        "connection failed".to_string()
    } else {
        let mut text = error.to_string();
        if let Some(url) = error.url() {
            text = text.replace(url.as_str(), "<request>");
        }
        text
    };
    FinanceError::Transport {
        source_name: source_name.into(),
        detail,
    }
}

/// Read a response as `(status, body)`, mapping transport failures.
pub(crate) fn read(
    source_name: &str,
    response: Result<reqwest::blocking::Response, reqwest::Error>,
) -> Result<(u16, String), FinanceError> {
    let response = response.map_err(|error| transport(source_name, &error))?;
    let status = response.status().as_u16();
    let body = response
        .text()
        .map_err(|error| transport(source_name, &error))?;
    Ok((status, body))
}

/// `key=value&…` with form-encoded values, in the order given.
pub(crate) fn query(pairs: &[(&str, &str)]) -> String {
    pairs
        .iter()
        .map(|(key, value)| {
            format!(
                "{key}={}",
                url::form_urlencoded::byte_serialize(value.as_bytes()).collect::<String>()
            )
        })
        .collect::<Vec<_>>()
        .join("&")
}

/// `YYYY-MM-DD` shape check for dates that end up in a URL.
pub(crate) fn is_iso_date(value: &str) -> bool {
    value.len() == 10
        && value.bytes().enumerate().all(|(index, byte)| match index {
            4 | 7 => byte == b'-',
            _ => byte.is_ascii_digit(),
        })
}
