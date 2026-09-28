//! Read-only finance plane.
//!
//! Heiwa reads a user's brokerage accounts and public market data into local
//! text truth, then derives portfolio read models from it. Nothing in this
//! crate can place an order, move money, or change a brokerage account: the
//! brokerage client only issues `GET` requests against read endpoints.

pub mod analytics;
mod http;
pub mod market;
pub mod model;
pub mod snaptrade;
pub mod store;
pub mod summary;
pub mod sync;

/// Every failure a finance read can surface. Messages are safe to show and
/// log: they never carry a credential.
#[derive(Debug, thiserror::Error)]
pub enum FinanceError {
    #[error("{0}")]
    Auth(String),
    #[error("rate limited by {source_name}; retry after {retry_after_seconds:?}s")]
    RateLimited {
        source_name: String,
        retry_after_seconds: Option<u64>,
    },
    #[error("{source_name} is busy syncing with the brokerage; try again shortly")]
    Busy { source_name: String },
    #[error("{source_name} returned HTTP {status}: {detail}")]
    Http {
        source_name: String,
        status: u16,
        detail: String,
    },
    #[error("{source_name} could not be reached: {detail}")]
    Transport { source_name: String, detail: String },
    #[error("{source_name} sent a response Heiwa could not read: {detail}")]
    Decode { source_name: String, detail: String },
    #[error("{0}")]
    Store(String),
}

#[cfg(test)]
mod test_support {
    /// A loopback HTTP server that answers each connection with the next
    /// canned response and records the raw request it received.
    pub(crate) struct Stub {
        pub url: String,
        pub requests: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
        pub thread: std::thread::JoinHandle<()>,
    }

    pub(crate) fn stub(responses: Vec<(u16, &'static str, String)>) -> Stub {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let requests = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen = requests.clone();
        listener.set_nonblocking(true).unwrap();
        let thread = std::thread::spawn(move || {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            for (status, extra_headers, body) in responses {
                // A client that never calls must fail the test, not hang it.
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(_) if std::time::Instant::now() < deadline => {
                            std::thread::sleep(std::time::Duration::from_millis(10))
                        }
                        Err(_) => return,
                    }
                };
                stream.set_nonblocking(false).unwrap();
                let mut raw = Vec::new();
                let mut buffer = [0u8; 4096];
                while !raw.windows(4).any(|window| window == b"\r\n\r\n") {
                    let read = stream.read(&mut buffer).unwrap();
                    if read == 0 {
                        break;
                    }
                    raw.extend_from_slice(&buffer[..read]);
                }
                seen.lock()
                    .unwrap()
                    .push(String::from_utf8_lossy(&raw).to_string());
                let response = format!(
                    "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{extra_headers}\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).unwrap();
            }
        });
        Stub {
            url,
            requests,
            thread,
        }
    }

    pub(crate) fn request_line(raw: &str) -> &str {
        raw.lines().next().unwrap_or_default()
    }

    pub(crate) fn header<'a>(raw: &'a str, name: &str) -> Option<&'a str> {
        raw.lines().find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.trim().eq_ignore_ascii_case(name).then(|| value.trim())
        })
    }
}
