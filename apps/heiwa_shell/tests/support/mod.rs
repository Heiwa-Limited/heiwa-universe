//! Shared harness for the fresh-install (L1) and first-run (L2) contracts.
//!
//! Both drive the shipped `heiwa` binary as a child process against a
//! loopback mock. Keeping one implementation means the two contracts cannot
//! drift into disagreeing about what "a fresh machine" is.

#![allow(dead_code)]

use heiwa_provider::registry::{
    AccountRegistry, AccountStatus, Credential, DetectedModel, InventoryTruth, ProviderAccount,
};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc as std_mpsc;
use std::sync::{Arc, Mutex};
use std::thread;

const PROVIDER_CLIS: &[&str] = &["claude", "codex", "gemini", "ollama", "antigravity"];
pub const HARNESS_API_KEY: &str = "sk-ant-harness-key";

/// A loopback server that answers `POST /v1/messages` with a canned
/// Messages API SSE stream, and `GET /v1/models` with a model list.
///
/// Hand-rolled over `TcpListener` rather than pulling in an HTTP test
/// server: the harness needs one request shape, and a new dependency in the
/// workspace is a bigger commitment than forty lines of socket handling.
pub struct MockProvider {
    pub base_url: String,
    requests: std_mpsc::Receiver<Request>,
    seen: Arc<Mutex<Vec<Request>>>,
}

/// One request the mock served, kept whole so tests can assert on headers.
#[derive(Debug, Clone)]
pub struct Request {
    pub line: String,
    pub headers: HashMap<String, String>,
    pub body: String,
}

impl MockProvider {
    pub fn start() -> Self {
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
    pub fn next_request(&self) -> Request {
        self.requests
            .recv_timeout(std::time::Duration::from_secs(30))
            .expect("mock provider received a request")
    }

    /// Every request served so far.
    pub fn all_requests(&self) -> Vec<Request> {
        self.seen.lock().expect("recorder lock").clone()
    }

    /// The first request whose start-line begins with `prefix`, if any.
    pub fn request_matching(&self, prefix: &str) -> Option<Request> {
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
pub fn single_api_key_registry() -> AccountRegistry {
    AccountRegistry {
        accounts: vec![ProviderAccount {
            account_id: "anthropic-api-harness".to_string(),
            provider: "anthropic".to_string(),
            credential: Credential::ApiKey,
            rate_group: "anthropic_api".to_string(),
            status: AccountStatus::Connected,
            models: vec![DetectedModel {
                model_id: "claude-opus-5".to_string(),
                provider_model_id: "claude-opus-5".to_string(),
                provider: "anthropic".to_string(),
                account_id: "anthropic-api-harness".to_string(),
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
pub fn assert_no_provider_cli_reachable(install: &FreshInstall) {
    // Resolve exactly as the child does: same home, same runtime root, same
    // PATH, same system directories. Passing empty lists here instead would
    // make the assertion true by construction — it could never fail, and the
    // child would go on finding /usr/local/bin/claude.
    let path = install.empty_bin.to_string_lossy().into_owned();
    let system_dirs: Vec<&str> = HARNESS_BIN_DIRS
        .split(':')
        .filter(|d| !d.is_empty())
        .collect();
    for binary in PROVIDER_CLIS {
        assert!(
            heiwa_provider::resolve_command_in(
                binary,
                &install.home,
                &install.state_dir,
                &path,
                &system_dirs,
            )
            .is_none(),
            "fresh-install harness requires no reachable provider CLI, found `{binary}`"
        );
    }
}

/// The child probes only its `PATH`. `HEIWA_BIN_DIRS` set empty removes the
/// built-in system directories, which an emptied `PATH` alone does not.
pub const HARNESS_BIN_DIRS: &str = "";

/// Path to the `heiwa` binary this test run built.
///
/// Cargo hands the integration test its own executable path; the binary under
/// test is its sibling in the same profile directory.
pub fn heiwa_binary() -> PathBuf {
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
pub struct FreshInstall {
    /// Whether the child sees a provider key in its environment. An install
    /// that starts with one is not the state a first-run walkthrough begins
    /// from — discovery adopts it and onboarding is complete before the user
    /// has done anything.
    supply_api_key: std::cell::Cell<bool>,
    _temp: tempfile::TempDir,
    pub home: PathBuf,
    pub state_dir: PathBuf,
    pub empty_bin: PathBuf,
}

impl FreshInstall {
    pub fn new(registry: &AccountRegistry) -> Self {
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
            supply_api_key: std::cell::Cell::new(true),
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
    /// An install with nothing in it: no accounts, no identity. What a
    /// user actually starts from, and what L2's contract begins at.
    pub fn empty() -> Self {
        let install = Self::new(&AccountRegistry::default());
        install.supply_api_key.set(false);
        install
    }

    /// The user makes a provider key available to the application.
    pub fn provide_api_key(&self) {
        self.supply_api_key.set(true);
    }

    pub fn run(&self, mock: &MockProvider, args: &[&str]) -> Run {
        self.run_with(mock, args, true)
    }

    /// Run with no state root at all, to exercise the gap that precedes
    /// every other one.
    pub fn run_without_state_root(&self, mock: &MockProvider, args: &[&str]) -> Run {
        self.run_with(mock, args, false)
    }

    fn run_with(&self, mock: &MockProvider, args: &[&str], state_root: bool) -> Run {
        let mut command = Command::new(heiwa_binary());
        command.args(args).env_clear().env("PATH", &self.empty_bin);
        if state_root {
            command
                .env("HOME", &self.home)
                .env("HEIWA_HOME", &self.state_dir)
                .env("HEIWA_STATE_DIR", &self.state_dir);
        }
        command
            // No keychain exists on a fresh container install; the provider's
            // own conventional variable carries the key.
            .envs(
                self.supply_api_key
                    .get()
                    .then_some(("ANTHROPIC_API_KEY", HARNESS_API_KEY)),
            )
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
            // No provider CLI is reachable. An emptied PATH is not enough:
            // discovery also probes /opt/homebrew/bin and /usr/local/bin, so
            // on a developer machine the real `claude` binary is found and
            // registered, and the harness stops modelling a fresh install.
            .env("HEIWA_BIN_DIRS", HARNESS_BIN_DIRS);
        Run::from(command.output().expect("run the heiwa binary"))
    }
}

/// One child-process run, with its streams decoded.
pub struct Run {
    pub status: std::process::ExitStatus,
    pub stdout: String,
    pub stderr: String,
}

impl From<std::process::Output> for Run {
    fn from(output: std::process::Output) -> Self {
        Self {
            status: output.status,
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        }
    }
}

impl Run {
    /// Both streams, for assertion messages — a first-run report can land on
    /// either, and a failure message that shows only one hides the reason.
    pub fn text(&self) -> String {
        format!("stdout: {}\nstderr: {}", self.stdout, self.stderr)
    }
}
