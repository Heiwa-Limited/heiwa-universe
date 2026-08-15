//! L1 fresh-install contract.
//!
//! The roadmap's acceptance criterion for L1: *a test harness with no
//! provider CLI on `PATH` and a single API key completes a turn end to end.*
//!
//! The contract test drives the **shipped `heiwa` binary** as a subprocess.
//! That matters more than convenience: a test that constructs an adapter
//! directly proves the adapter works, not that a user's turn reaches it. An
//! earlier version of this file did exactly that, and missed a defect where
//! the shell dropped every direct-API model before routing ever ran — the
//! adapter was fine and the product was broken.
//!
//! A child process also gives real isolation: its own `PATH`, `HOME`, and
//! state root, set at spawn rather than mutated into this process's global
//! environment where parallel tests would race.
//!
//! Everything is hermetic — a temp state root, an emptied `PATH`, and a
//! loopback mock speaking the Anthropic Messages API wire format. No network,
//! no keychain, no installed CLI. That is the machine a stranger installs
//! Heiwa on.

use heiwa_provider::health::{FleetHealth, HealthState};
use heiwa_provider::registry::{
    AccountRegistry, AccountStatus, Credential, DetectedModel, InventoryTruth, ProviderAccount,
};
use heiwa_provider::routing::{resolve_adapter_with, routable_api_key_account};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc as std_mpsc;
use std::sync::{Arc, Mutex};
use std::thread;

const PROVIDER_CLIS: &[&str] = &["claude", "codex", "gemini", "ollama", "antigravity"];
const HARNESS_API_KEY: &str = "sk-ant-harness-key";

/// A loopback server that answers `POST /v1/messages` with a canned
/// Messages API SSE stream, and `GET /v1/models` with a model list.
///
/// Hand-rolled over `TcpListener` rather than pulling in an HTTP test
/// server: the harness needs one request shape, and a new dependency in the
/// workspace is a bigger commitment than forty lines of socket handling.
struct MockProvider {
    base_url: String,
    requests: std_mpsc::Receiver<Request>,
    seen: Arc<Mutex<Vec<Request>>>,
}

/// One request the mock served, kept whole so tests can assert on headers.
#[derive(Debug, Clone)]
struct Request {
    line: String,
    headers: HashMap<String, String>,
    body: String,
}

impl MockProvider {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let port = listener.local_addr().expect("addr").port();
        let (tx, requests) = std_mpsc::channel();
        let seen: Arc<Mutex<Vec<Request>>> = Arc::new(Mutex::new(Vec::new()));
        let recorder = Arc::clone(&seen);

        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { break };
                let tx = tx.clone();
                let recorder = Arc::clone(&recorder);
                // One thread per connection: the runtime opens the models
                // probe and the turn concurrently.
                thread::spawn(move || {
                    let _ = handle(stream, &tx, &recorder);
                });
            }
        });

        Self {
            base_url: format!("http://127.0.0.1:{port}"),
            requests,
            seen,
        }
    }

    /// Block until the mock has served a request, and return it.
    fn next_request(&self) -> Request {
        self.requests
            .recv_timeout(std::time::Duration::from_secs(30))
            .expect("mock provider received a request")
    }

    /// Every request served so far.
    fn all_requests(&self) -> Vec<Request> {
        self.seen.lock().expect("recorder lock").clone()
    }

    /// The first request whose start-line begins with `prefix`, if any.
    fn request_matching(&self, prefix: &str) -> Option<Request> {
        self.all_requests()
            .into_iter()
            .find(|request| request.line.starts_with(prefix))
    }
}

fn handle(
    mut stream: TcpStream,
    tx: &std_mpsc::Sender<Request>,
    seen: &Mutex<Vec<Request>>,
) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;

    let mut headers = HashMap::new();
    loop {
        let mut header = String::new();
        reader.read_line(&mut header)?;
        if header.trim().is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
        }
    }

    let content_length: usize = headers
        .get("content-length")
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    let mut body = vec![0u8; content_length];
    if content_length > 0 {
        std::io::Read::read_exact(&mut reader, &mut body)?;
    }

    let request = Request {
        line: request_line.clone(),
        headers,
        body: String::from_utf8_lossy(&body).into_owned(),
    };
    seen.lock().expect("recorder lock").push(request.clone());
    let _ = tx.send(request);

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
fn assert_no_provider_cli_reachable(home: &Path) {
    for binary in PROVIDER_CLIS {
        assert!(
            heiwa_provider::resolve_command_in(binary, home, &home.join(".heiwa"), "", &[])
                .is_none(),
            "fresh-install harness requires no reachable provider CLI, found `{binary}`"
        );
    }
}

/// Path to the `heiwa` binary this test run built.
///
/// Cargo hands the integration test its own executable path; the binary under
/// test is its sibling in the same profile directory.
fn heiwa_binary() -> PathBuf {
    let mut dir = std::env::current_exe().expect("test executable path");
    dir.pop(); // .../deps
    if dir.ends_with("deps") {
        dir.pop();
    }
    let binary = dir.join(format!("heiwa{}", std::env::consts::EXE_SUFFIX));
    assert!(
        binary.exists(),
        "the heiwa binary must be built before this test: {}",
        binary.display()
    );
    binary
}

/// A fresh install on disk: temp state root, one API key account, no CLI.
struct FreshInstall {
    _temp: tempfile::TempDir,
    home: PathBuf,
    state_dir: PathBuf,
    empty_bin: PathBuf,
}

impl FreshInstall {
    fn new(registry: &AccountRegistry) -> Self {
        let temp = tempfile::tempdir().expect("tempdir");
        let home = temp.path().to_path_buf();
        let state_dir = home.join(".heiwa");
        let empty_bin = home.join("empty-bin");
        std::fs::create_dir_all(&state_dir).expect("state dir");
        std::fs::create_dir_all(&empty_bin).expect("empty bin");
        std::fs::write(
            state_dir.join("accounts.json"),
            serde_json::to_string_pretty(registry).expect("serialize registry"),
        )
        .expect("write registry");

        Self {
            _temp: temp,
            home,
            state_dir,
            empty_bin,
        }
    }

    /// Run `heiwa <args>` against this install and the given mock provider.
    ///
    /// The environment is built for the child rather than mutated into this
    /// process, so parallel tests cannot observe each other's `PATH` or
    /// `HOME`, and a panic cannot leave the harness's environment behind.
    fn run(&self, mock: &MockProvider, args: &[&str]) -> std::process::Output {
        Command::new(heiwa_binary())
            .args(args)
            .env_clear()
            .env("PATH", &self.empty_bin)
            .env("HOME", &self.home)
            .env("HEIWA_HOME", &self.state_dir)
            .env("HEIWA_STATE_DIR", &self.state_dir)
            // No keychain exists on a fresh container install; the provider's
            // own conventional variable carries the key.
            .env("ANTHROPIC_API_KEY", HARNESS_API_KEY)
            // Point the direct-API adapter at the loopback mock the same way
            // a user points it at a gateway.
            .env("ANTHROPIC_BASE_URL", &mock.base_url)
            .env("HEIWA_NONINTERACTIVE", "1")
            // A fresh install has no local runtime either. An emptied `PATH`
            // cannot hide one — Ollama is discovered over HTTP on a fixed
            // loopback port, so on a developer machine it would be found and
            // win the route on price, and the harness would silently stop
            // testing the direct-API path it exists to test. Port 1 never
            // listens.
            .env("HEIWA_OLLAMA_BASE", "http://127.0.0.1:1")
            .output()
            .expect("run the heiwa binary")
    }
}

/// THE L1 CONTRACT: no provider CLI, one API key, a turn completes —
/// through the shipped binary's own turn path.
#[test]
fn fresh_install_with_one_api_key_and_no_cli_completes_a_turn() {
    let mock = MockProvider::start();
    let install = FreshInstall::new(&single_api_key_registry());
    assert_no_provider_cli_reachable(&install.home);

    let output = install.run(&mock, &["ask", "Say hello."]);

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(
        output.status.success(),
        "heiwa ask failed ({}):\nstdout: {stdout}\nstderr: {stderr}",
        output.status
    );

    // The model's text reached the user through the real turn pipeline.
    assert!(
        stdout.contains("Heiwa is running."),
        "the assistant's text did not reach stdout:\nstdout: {stdout}\nstderr: {stderr}"
    );

    // And the turn really went to the Messages API with the supplied key.
    let turn = mock
        .request_matching("POST /v1/messages")
        .unwrap_or_else(|| {
            panic!(
                "no Messages API request was made; saw {:?}",
                mock.all_requests()
            )
        });
    assert_eq!(
        turn.headers.get("x-api-key").map(String::as_str),
        Some(HARNESS_API_KEY),
        "the request did not carry the user's key"
    );
    assert!(turn.body.contains("\"stream\":true"));
    assert!(turn.body.contains("Say hello."));
}

/// The turn must reach the provider *because* routing chose the API key,
/// not because a CLI happened to be installed.
#[test]
fn the_completed_turn_routed_through_the_users_api_key() {
    let mock = MockProvider::start();
    let install = FreshInstall::new(&single_api_key_registry());

    let output = install.run(&mock, &["route", "preview", "Say hello."]);
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();

    assert!(
        output.status.success(),
        "route preview failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("claude-opus-5"),
        "the direct-API model was dropped before routing:\n{stdout}"
    );
}

/// Model inventory is discovered from the provider, not carried in-tree.
#[test]
fn fresh_install_discovers_model_inventory_from_the_provider() {
    let mock = MockProvider::start();
    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");

    let models = runtime
        .block_on(heiwa_provider::providers::anthropic_api::discover_models(
            HARNESS_API_KEY,
            &mock.base_url,
            "anthropic-api-1",
            "anthropic_api",
        ))
        .expect("discovery succeeds");

    assert_eq!(models.len(), 1);
    assert_eq!(models[0].provider_model_id, "claude-opus-5");
    assert_eq!(models[0].inventory_truth, InventoryTruth::Verified);

    let probe = mock.next_request();
    assert!(probe.line.starts_with("GET /v1/models"));
    assert_eq!(
        probe.headers.get("x-api-key").map(String::as_str),
        Some(HARNESS_API_KEY)
    );
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
    registry.accounts[0].status = AccountStatus::Error("Invalid API key".to_string());

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

    let adapter = resolve_adapter_with(&registry, "claude", "claude-opus-5", Some(&mock.base_url))
        .expect("an adapter resolves from the API key alone");
    assert_eq!(
        adapter.supported_models(),
        vec!["claude-opus-5".to_string()]
    );
}
