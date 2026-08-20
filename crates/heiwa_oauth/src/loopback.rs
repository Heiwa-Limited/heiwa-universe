//! One-shot loopback redirect listener.
//!
//! Google retired both the out-of-band flow and custom URI schemes on desktop,
//! so a loopback HTTP listener is the only supported redirect for an installed
//! application. It binds an ephemeral port, answers exactly one request, and
//! closes (AD-18) — a listener that outlives the exchange is an open local port
//! any other process on the machine can talk to.

use std::io::{BufRead, BufReader, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::time::Duration;
use url::Url;

use crate::OAuthError;

/// Bound listener, before the browser has been sent anywhere.
pub struct LoopbackListener {
    listener: TcpListener,
    redirect_uri: String,
}

impl LoopbackListener {
    /// Bind 127.0.0.1 on a port the OS chooses.
    pub fn bind() -> Result<Self, OAuthError> {
        let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
            .map_err(OAuthError::Listener)?;
        let port = listener.local_addr().map_err(OAuthError::Listener)?.port();
        Ok(Self {
            listener,
            redirect_uri: format!("http://127.0.0.1:{port}"),
        })
    }

    /// The address to register as `redirect_uri`. Only known after binding,
    /// which is why the authorization URL cannot be built first.
    pub fn redirect_uri(&self) -> &str {
        &self.redirect_uri
    }

    /// Block until the provider redirects the browser here, then return the
    /// authorization code. Consumes the listener: one request, then closed.
    pub fn wait_for_code(
        self,
        expected_state: &str,
        timeout: Duration,
    ) -> Result<String, OAuthError> {
        self.listener
            .set_nonblocking(true)
            .map_err(OAuthError::Listener)?;

        let deadline = std::time::Instant::now() + timeout;
        let (mut stream, _) = loop {
            match self.listener.accept() {
                Ok(connection) => break connection,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    let now = std::time::Instant::now();
                    if now >= deadline {
                        return Err(OAuthError::CallbackTimeout);
                    }
                    std::thread::sleep(
                        deadline
                            .saturating_duration_since(now)
                            .min(Duration::from_millis(5)),
                    );
                }
                Err(error) => return Err(OAuthError::Listener(error)),
            }
        };
        stream
            .set_nonblocking(false)
            .map_err(OAuthError::Listener)?;
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            return Err(OAuthError::CallbackTimeout);
        }
        stream
            .set_read_timeout(Some(remaining))
            .map_err(OAuthError::Listener)?;

        let target = read_request_target(&mut stream)?;
        let result = extract_code(&target, expected_state);

        // Answer before propagating: a browser left on a hung connection shows
        // the user a failure even when the exchange succeeded.
        let body = match &result {
            Ok(_) => "Heiwa is connected. You can close this tab.",
            Err(_) => "Heiwa could not complete the connection. Return to the app.",
        };
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\n\
             Content-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        let _ = stream.write_all(response.as_bytes());
        let _ = stream.flush();

        result
    }
}

fn read_request_target(stream: &mut TcpStream) -> Result<String, OAuthError> {
    let mut reader = BufReader::new(stream);
    let mut request_line = String::new();
    reader
        .read_line(&mut request_line)
        .map_err(OAuthError::Listener)?;

    // "GET /?code=...&state=... HTTP/1.1"
    request_line
        .split_whitespace()
        .nth(1)
        .map(str::to_string)
        .ok_or(OAuthError::MalformedCallback)
}

fn extract_code(target: &str, expected_state: &str) -> Result<String, OAuthError> {
    let url = Url::parse(&format!("http://127.0.0.1{target}"))
        .map_err(|_| OAuthError::MalformedCallback)?;

    let mut code = None;
    let mut state = None;
    let mut provider_error = None;
    for (key, value) in url.query_pairs() {
        match key.as_ref() {
            "code" => code = Some(value.into_owned()),
            "state" => state = Some(value.into_owned()),
            "error" => provider_error = Some(value.into_owned()),
            _ => {}
        }
    }

    if let Some(error) = provider_error {
        return Err(OAuthError::AuthorizationDenied { reason: error });
    }

    // Check state before touching the code: an attacker-supplied callback is
    // exactly the case where the code must not be used.
    match state.as_deref() {
        Some(actual) if actual == expected_state => {}
        Some(_) => return Err(OAuthError::StateMismatch),
        None => return Err(OAuthError::MalformedCallback),
    }

    code.ok_or(OAuthError::MalformedCallback)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binds_loopback_on_an_ephemeral_port() {
        let listener = LoopbackListener::bind().unwrap();
        let uri = listener.redirect_uri();
        assert!(uri.starts_with("http://127.0.0.1:"), "got {uri}");
        let port: u16 = uri.rsplit(':').next().unwrap().parse().unwrap();
        assert_ne!(port, 0, "the OS must have assigned a real port");
    }

    #[test]
    fn two_listeners_do_not_collide() {
        let a = LoopbackListener::bind().unwrap();
        let b = LoopbackListener::bind().unwrap();
        assert_ne!(a.redirect_uri(), b.redirect_uri());
    }

    #[test]
    fn accepts_a_matching_state() {
        assert_eq!(
            extract_code("/?code=abc123&state=xyz", "xyz").unwrap(),
            "abc123"
        );
    }

    #[test]
    fn rejects_a_forged_state() {
        assert!(matches!(
            extract_code("/?code=abc123&state=attacker", "xyz"),
            Err(OAuthError::StateMismatch)
        ));
    }

    #[test]
    fn rejects_a_callback_with_no_state_even_when_it_carries_a_code() {
        assert!(matches!(
            extract_code("/?code=abc123", "xyz"),
            Err(OAuthError::MalformedCallback)
        ));
    }

    #[test]
    fn surfaces_a_provider_error_ahead_of_a_missing_code() {
        assert!(matches!(
            extract_code("/?error=access_denied&state=xyz", "xyz"),
            Err(OAuthError::AuthorizationDenied { reason }) if reason == "access_denied"
        ));
    }

    #[test]
    fn a_denial_is_reported_even_when_state_is_absent() {
        // The provider does not always echo state on the error path, and a
        // user who clicked "deny" should not be told the callback was corrupt.
        assert!(matches!(
            extract_code("/?error=access_denied", "xyz"),
            Err(OAuthError::AuthorizationDenied { .. })
        ));
    }

    #[test]
    fn callback_timeout_does_not_wait_for_a_late_connection() {
        let listener = LoopbackListener::bind().unwrap();
        let address = listener
            .redirect_uri()
            .trim_start_matches("http://")
            .to_string();
        let late = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(150));
            let _ = TcpStream::connect(address);
        });
        let started = std::time::Instant::now();

        let error = listener
            .wait_for_code("expected", Duration::from_millis(20))
            .expect_err("listener must honor its deadline");

        assert!(matches!(error, OAuthError::CallbackTimeout));
        assert!(
            started.elapsed() < Duration::from_millis(100),
            "timeout waited for a late connection: {:?}",
            started.elapsed()
        );
        late.join().unwrap();
    }
}
