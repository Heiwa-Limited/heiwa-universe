//! L1 fresh-install contract.
//!
//! The roadmap's acceptance criterion for L1: *a test harness with no
//! provider CLI on `PATH` and a single API key completes a turn end to end.*
//!
//! Everything here is hermetic — a temp state root, a scrubbed `PATH`, and a
//! loopback mock speaking the Anthropic Messages API wire format. No network,
//! no keychain, no installed CLI. That is the point: this is the machine a
//! stranger installs Heiwa on.

use heiwa_provider::adapter::{Message, ProviderAdapter, Role, StreamEvent};
use heiwa_provider::health::{FleetHealth, HealthState};
use heiwa_provider::providers::anthropic_api::AnthropicApiAdapter;
use heiwa_provider::registry::{
    AccountRegistry, AccountStatus, Credential, DetectedModel, InventoryTruth, ProviderAccount,
};
use heiwa_provider::routing::{resolve_adapter_with, routable_api_key_account};
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc as std_mpsc;
use std::thread;
use tokio::sync::mpsc;

const PROVIDER_CLIS: &[&str] = &["claude", "codex", "gemini", "ollama", "antigravity"];

/// A loopback server that answers `POST /v1/messages` with a canned
/// Messages API SSE stream, and `GET /v1/models` with a model list.
///
/// Hand-rolled over `TcpListener` rather than pulling in an HTTP test
/// server: the harness needs one request shape, and a new dependency in the
/// workspace is a bigger commitment than forty lines of socket handling.
struct MockProvider {
    base_url: String,
    requests: std_mpsc::Receiver<String>,
}

impl MockProvider {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let port = listener.local_addr().expect("addr").port();
        let (tx, requests) = std_mpsc::channel();

        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { break };
                if handle(stream, &tx).is_err() {
                    break;
                }
            }
        });

        Self {
            base_url: format!("http://127.0.0.1:{port}"),
            requests,
        }
    }

    /// The most recent request line + body the server saw.
    fn last_request(&self) -> String {
        self.requests
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect("mock provider received a request")
    }
}

fn handle(mut stream: TcpStream, tx: &std_mpsc::Sender<String>) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;

    let mut content_length = 0usize;
    loop {
        let mut header = String::new();
        reader.read_line(&mut header)?;
        if header.trim().is_empty() {
            break;
        }
        if let Some(value) = header.to_ascii_lowercase().strip_prefix("content-length:") {
            content_length = value.trim().parse().unwrap_or(0);
        }
    }

    let mut body = vec![0u8; content_length];
    if content_length > 0 {
        std::io::Read::read_exact(&mut reader, &mut body)?;
    }
    let body = String::from_utf8_lossy(&body).into_owned();
    let _ = tx.send(format!("{request_line}{body}"));

    if request_line.starts_with("GET /v1/models") {
        let payload = r#"{"data":[{"id":"claude-opus-5","display_name":"Claude Opus 5","max_input_tokens":1000000,"max_tokens":128000}],"has_more":false}"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
            payload.len()
        )?;
        return stream.flush();
    }

    // A minimal but complete Messages API stream: usage, two text deltas,
    // then the terminal message_stop.
    let sse = concat!(
        "event: message_start\n",
        "data: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":11,\"output_tokens\":0}}}\n\n",
        "event: content_block_start\n",
        "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Heiwa \"}}\n\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"is running.\"}}\n\n",
        "event: content_block_stop\n",
        "data: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
        "event: message_delta\n",
        "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":4}}\n\n",
        "event: message_stop\n",
        "data: {\"type\":\"message_stop\"}\n\n",
    );
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{sse}",
        sse.len()
    )?;
    stream.flush()
}

/// A registry holding exactly one credential: the fresh-install shape.
fn single_api_key_registry() -> AccountRegistry {
    AccountRegistry {
        accounts: vec![ProviderAccount {
            account_id: "anthropic-api-1".to_string(),
            provider: "anthropic".to_string(),
            credential: Credential::ApiKey,
            rate_group: "anthropic_api".to_string(),
            status: AccountStatus::Connected,
            models: vec![DetectedModel {
                model_id: "claude-opus-5".to_string(),
                provider_model_id: "claude-opus-5".to_string(),
                provider: "anthropic".to_string(),
                account_id: "anthropic-api-1".to_string(),
                rate_group: "anthropic_api".to_string(),
                capability_class: 5,
                context_window: 1_000_000,
                supports_streaming: true,
                supports_tools: true,
                supports_vision: true,
                supports_audio: false,
                cost_per_1k_input: 0.0,
                cost_per_1k_output: 0.0,
                inventory_truth: InventoryTruth::Verified,
            }],
        }],
    }
}

/// Assert no provider CLI is reachable, the way the runtime itself looks.
///
/// Uses the same resolution production uses, with the system bin directories
/// emptied. Scrubbing `PATH` alone is not enough: discovery also probes
/// `/opt/homebrew/bin` and friends, so on a developer Mac the real `claude`
/// binary would still be found and the harness would be testing the CLI path
/// while claiming to test the fresh-install one.
fn assert_no_provider_cli_reachable(home: &std::path::Path) {
    for binary in PROVIDER_CLIS {
        assert!(
            heiwa_provider::resolve_command_in(binary, home, &home.join(".heiwa"), "", &[])
                .is_none(),
            "fresh-install harness requires no reachable provider CLI, found `{binary}`"
        );
    }
}

/// Run `body` with a scrubbed `PATH` and a temp state root, so the harness
/// cannot see the developer machine's CLIs or state.
fn with_fresh_install<T>(body: impl FnOnce(&std::path::Path) -> T) -> T {
    use std::sync::{Mutex, OnceLock};
    // Env is process-global; serialize so parallel tests cannot interleave.
    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let _guard = ENV_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|e| e.into_inner());

    let temp = tempfile::tempdir().expect("tempdir");
    let empty_bin = temp.path().join("empty-bin");
    std::fs::create_dir_all(&empty_bin).expect("create empty bin");

    let prior_path = std::env::var_os("PATH");
    let prior_home = std::env::var_os("HOME");
    let prior_heiwa_home = std::env::var_os("HEIWA_HOME");

    std::env::set_var("PATH", &empty_bin);
    std::env::set_var("HOME", temp.path());
    std::env::set_var("HEIWA_HOME", temp.path().join(".heiwa"));

    let result = body(temp.path());

    match prior_path {
        Some(value) => std::env::set_var("PATH", value),
        None => std::env::remove_var("PATH"),
    }
    match prior_home {
        Some(value) => std::env::set_var("HOME", value),
        None => std::env::remove_var("HOME"),
    }
    match prior_heiwa_home {
        Some(value) => std::env::set_var("HEIWA_HOME", value),
        None => std::env::remove_var("HEIWA_HOME"),
    }
    result
}

/// THE L1 CONTRACT: no provider CLI, one API key, a turn completes.
#[test]
fn fresh_install_with_one_api_key_and_no_cli_completes_a_turn() {
    let mock = MockProvider::start();
    let registry = single_api_key_registry();

    let (events, request) = with_fresh_install(|home| {
        assert_no_provider_cli_reachable(home);

        // Routing must pick the direct-API adapter: there is no CLI to fall
        // back to, and the user's key is the only thing that can serve.
        let account =
            routable_api_key_account(&registry, "claude").expect("the API key account is routable");
        assert_eq!(account.account_id, "anthropic-api-1");

        let adapter = AnthropicApiAdapter::new(&account.account_id, &mock.base_url)
            // The credential is supplied directly: a fresh install in a
            // container has no OS keychain.
            .with_api_key("sk-ant-harness-key")
            .with_models(vec!["claude-opus-5".to_string()]);

        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
        let prompt = vec![Message {
            role: Role::User,
            content: "Say hello.".to_string(),
        }];
        let events = runtime.block_on(async move {
            let (tx, mut rx) = mpsc::channel(32);
            let send = adapter.send("claude-opus-5", &prompt, tx);

            let collect = async move {
                let mut collected = Vec::new();
                while let Some(event) = rx.recv().await {
                    collected.push(event);
                }
                collected
            };
            let (result, collected) = tokio::join!(send, collect);
            result.expect("adapter send succeeds");
            collected
        });

        (events, mock.last_request())
    });

    // The turn produced assistant text and a terminal Done — a complete turn.
    let text: String = events
        .iter()
        .filter_map(|event| match event {
            StreamEvent::Token(token) => Some(token.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(text, "Heiwa is running.");

    match events.last() {
        Some(StreamEvent::Done(usage)) => {
            assert_eq!(usage.input_tokens, 11);
            assert_eq!(usage.output_tokens, 4);
        }
        other => panic!("turn did not end with Done: {other:?}"),
    }
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, StreamEvent::Error(_))),
        "turn produced an error event"
    );

    // The request really went to the Messages API with the supplied key.
    assert!(request.starts_with("POST /v1/messages"));
    assert!(request.contains("\"model\":\"claude-opus-5\""));
    assert!(request.contains("\"stream\":true"));
    assert!(request.contains("Say hello."));
}

/// Model inventory is discovered from the provider, not carried in-tree.
#[test]
fn fresh_install_discovers_model_inventory_from_the_provider() {
    let mock = MockProvider::start();
    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");

    let models = runtime
        .block_on(heiwa_provider::providers::anthropic_api::discover_models(
            "sk-ant-harness-key",
            &mock.base_url,
            "anthropic-api-1",
            "anthropic_api",
        ))
        .expect("discovery succeeds");

    assert_eq!(models.len(), 1);
    assert_eq!(models[0].provider_model_id, "claude-opus-5");
    assert_eq!(models[0].inventory_truth, InventoryTruth::Verified);
    assert!(mock.last_request().starts_with("GET /v1/models"));
}

/// Zero providers must be an actionable state, not a crash.
#[test]
fn a_fresh_install_with_no_accounts_is_actionable_rather_than_broken() {
    let fleet = FleetHealth::project(&[]);
    assert!(!fleet.has_routable_account());

    let guidance = fleet.guidance();
    assert!(guidance.contains("Connect a provider"));
    // The guidance must name paths a user without any CLI can actually take.
    assert!(guidance.contains("API key"));
}

/// An unusable credential is a routing constraint that names its reason.
#[test]
fn an_expired_credential_is_skipped_with_a_reason_rather_than_failing_the_app() {
    let mut registry = single_api_key_registry();
    registry.accounts[0].status = AccountStatus::Error("HTTP 401: Invalid API key".to_string());

    assert!(routable_api_key_account(&registry, "claude").is_none());

    let fleet = FleetHealth::project(&registry.accounts);
    assert!(!fleet.has_routable_account());
    let report = &fleet.reports[0];
    assert_eq!(report.state, HealthState::Unauthenticated);
    assert!(report.detail.contains("Invalid API key"));
    assert!(fleet.guidance().contains("anthropic-api-1"));
}

/// A rate-limited account is temporary, and says so.
#[test]
fn a_rate_limited_account_is_a_temporary_constraint() {
    let mut registry = single_api_key_registry();
    registry.accounts[0].status = AccountStatus::Error("HTTP 429 rate limit".to_string());

    let fleet = FleetHealth::project(&registry.accounts);
    assert_eq!(fleet.reports[0].state, HealthState::RateLimited);
    assert!(!fleet.reports[0].routable);
}

/// With no CLI installed, resolution still yields an adapter for the key.
#[test]
fn adapter_resolution_prefers_the_users_key_when_no_cli_exists() {
    let mock = MockProvider::start();
    let registry = single_api_key_registry();

    with_fresh_install(|home| {
        assert_no_provider_cli_reachable(home);
        let adapter =
            resolve_adapter_with(&registry, "claude", "claude-opus-5", Some(&mock.base_url))
                .expect("an adapter resolves from the API key alone");
        assert_eq!(
            adapter.supported_models(),
            vec!["claude-opus-5".to_string()]
        );
    });
}
