use anyhow::{anyhow, Result};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use heiwa_resource::{ResourcePolicy, ResourceSnapshot, ThermalPressure, WorkClass};
use serde::Serialize;
use serde_json::{json, Value};
use sha1::{Digest, Sha1};
use std::collections::HashSet;
use std::env;
use std::fs;
use std::net::SocketAddr;
use std::path::{Component, Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::Instant;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;
use tokio::time::{self, Duration};

pub(crate) const DEFAULT_PORT: u16 = 7474;
const HEARTBEAT_TTL_SECS: i64 = 120;
const WS_GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
const GITHUB_RELEASES_URL: &str = "https://github.com/Strategizing/heiwa-universe/releases";
const GITHUB_LATEST_RELEASE_API: &str =
    "https://api.github.com/repos/Strategizing/heiwa-universe/releases/latest";

#[derive(Debug, Clone, Serialize)]
pub(crate) struct LocalAppProbe {
    pub port: u16,
    pub url: String,
    pub reachable: bool,
    pub latency_ms: Option<u64>,
}

pub(crate) fn probe_local_app(port: u16) -> LocalAppProbe {
    let addr: SocketAddr = ([127, 0, 0, 1], port).into();
    let start = Instant::now();
    let reachable = std::net::TcpStream::connect_timeout(&addr, Duration::from_millis(200)).is_ok();
    LocalAppProbe {
        port,
        url: format!("http://127.0.0.1:{port}/"),
        reachable,
        latency_ms: reachable.then(|| start.elapsed().as_millis() as u64),
    }
}

pub async fn run(args: &[String]) -> Result<()> {
    match args.first().map(String::as_str) {
        Some("start") => start(&args[1..]).await,
        Some("update") => update(&args[1..]),
        Some("runtime") => runtime(&args[1..]),
        Some("status") => runtime_status(args),
        Some("--help") | Some("-h") | None => {
            if args.iter().any(|arg| arg == "--json") {
                runtime_status(args)
            } else {
                print_help();
                Ok(())
            }
        }
        Some(flag) if flag.starts_with("--") => runtime_status(args),
        Some(other) => Err(anyhow!("unknown app command: {other}")),
    }
}

fn update(args: &[String]) -> Result<()> {
    if has_flag(args, "--help") || has_flag(args, "-h") {
        print_update_help();
        return Ok(());
    }

    let dry_run = has_flag(args, "--dry-run");
    let source = flag_value(args, "--source").unwrap_or_else(|| "github".to_string());

    match source.as_str() {
        "github" => update_from_github_release(dry_run),
        "checkout" => update_from_checkout(dry_run),
        other => Err(anyhow!(
            "invalid --source value: {other} (expected github or checkout)"
        )),
    }
}

fn update_from_github_release(dry_run: bool) -> Result<()> {
    let install_root = heiwa_install::get_heiwa_dir();
    println!("heiwa app update");
    println!("  source_mode: github-release");
    println!("  source: {GITHUB_RELEASES_URL}");
    println!("  release_api: {GITHUB_LATEST_RELEASE_API}");
    println!("  platform: {}", github_release_platform());
    println!(
        "  target: {}",
        install_root.join("bin").join("heiwa").display()
    );
    println!("  restart_policy: prompt-before-restart");
    if dry_run {
        println!("  dry_run: true");
        return Ok(());
    }

    Err(anyhow!(
        "GitHub release update is not implemented until release asset verification is wired; use --dry-run or --source checkout for dev-only reinstall"
    ))
}

fn update_from_checkout(dry_run: bool) -> Result<()> {
    let repo_root = find_repo_root(env::current_dir()?)
        .ok_or_else(|| anyhow!("heiwa app update must run from a heiwa-universe checkout"))?;
    let shell_manifest = repo_root
        .join("apps")
        .join("heiwa_shell")
        .join("Cargo.toml");
    if !shell_manifest.is_file() {
        return Err(anyhow!(
            "heiwa app update could not find apps/heiwa_shell/Cargo.toml under {}",
            repo_root.display()
        ));
    }

    let install_root = heiwa_install::get_heiwa_dir();
    let mut command = Command::new("cargo");
    command
        .arg("install")
        .arg("--path")
        .arg(repo_root.join("apps").join("heiwa_shell"))
        .arg("--root")
        .arg(&install_root)
        .arg("--locked")
        .arg("--force");

    println!("heiwa app update");
    println!("  source_mode: checkout-dev");
    println!("  source: {}", repo_root.display());
    println!("  official_source: GitHub Releases");
    println!(
        "  target: {}",
        install_root.join("bin").join("heiwa").display()
    );
    println!("  command: cargo install --path apps/heiwa_shell --root ~/.heiwa --locked --force");
    if dry_run {
        println!("  dry_run: true");
        return Ok(());
    }

    let status = command.status()?;
    if !status.success() {
        return Err(anyhow!("cargo install failed with status {status}"));
    }
    println!("  status: updated");
    Ok(())
}

fn github_release_platform() -> &'static str {
    match (env::consts::OS, env::consts::ARCH) {
        ("macos", "aarch64") => "macos-aarch64",
        ("linux", "x86_64") => "linux-x86_64",
        ("windows", "x86_64") => "windows-x86_64",
        _ => "unsupported",
    }
}

fn find_repo_root(start: PathBuf) -> Option<PathBuf> {
    for candidate in start.ancestors() {
        if candidate.join("HEIWA.md").is_file()
            && candidate
                .join("apps")
                .join("heiwa_shell")
                .join("Cargo.toml")
                .is_file()
        {
            return Some(candidate.to_path_buf());
        }
    }
    None
}

fn runtime(args: &[String]) -> Result<()> {
    match args.first().map(String::as_str) {
        Some("status") | None => runtime_status(args),
        Some("--help") | Some("-h") => {
            print_help();
            Ok(())
        }
        Some(other) => Err(anyhow!("unknown app runtime command: {other}")),
    }
}

async fn start(args: &[String]) -> Result<()> {
    if has_flag(args, "--help") || has_flag(args, "-h") {
        print_start_help();
        return Ok(());
    }

    let port = parse_port(args)?;
    let no_open = has_flag(args, "--no-open");
    let bind_addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = TcpListener::bind(bind_addr).await?;
    let local_addr = listener.local_addr()?;
    let url = format!("http://127.0.0.1:{}/", local_addr.port());
    let worker_id = format!("heiwa-app-{}", std::process::id());
    let started_at = Arc::new(chrono::Utc::now().to_rfc3339());

    write_app_heartbeat(&worker_id)?;
    let mut caffeinate = spawn_caffeinate();
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    tokio::spawn(heartbeat_loop(worker_id.clone(), shutdown_rx));

    if !no_open {
        open_url(&url)?;
    }

    println!("heiwa app start");
    println!("  url: {url}");
    println!("  worker_id: {worker_id}");
    println!(
        "  caffeinate: {}",
        caffeinate
            .as_ref()
            .map(|child| child.id().to_string())
            .unwrap_or_else(|| "not-started".to_string())
    );
    println!("  static: {}", cockpit_static_root().display());
    println!("  stop: SIGINT/SIGTERM");

    let signal = shutdown_signal();
    tokio::pin!(signal);

    loop {
        tokio::select! {
            _ = &mut signal => {
                break;
            }
            accepted = listener.accept() => {
                let (stream, _) = accepted?;
                let started_at = started_at.clone();
                tokio::spawn(async move {
                    if let Err(err) = handle_connection(stream, started_at).await {
                        eprintln!("heiwa app connection error: {err}");
                    }
                });
            }
        }
    }

    let _ = shutdown_tx.send(true);
    stop_caffeinate(&mut caffeinate);
    println!("heiwa app stopped");
    Ok(())
}

async fn heartbeat_loop(worker_id: String, mut shutdown: watch::Receiver<bool>) {
    let mut ticker = time::interval(Duration::from_secs(60));
    loop {
        tokio::select! {
            _ = ticker.tick() => {
                let _ = write_app_heartbeat(&worker_id);
            }
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    break;
                }
            }
        }
    }
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("install SIGTERM handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = terminate.recv() => {}
        }
    }

    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

fn runtime_status(args: &[String]) -> Result<()> {
    let status = RuntimeStatus::detect();
    if has_flag(args, "--json") {
        println!(
            "{}",
            json!({
                "command": "app runtime status",
                "state": status.state,
                "node": status.node,
                "cli_path": status.cli_path.display().to_string(),
                "state_dir": status.state_dir.display().to_string(),
                "transport": status.transport,
                "sidecar": status.sidecar,
                "keep_awake": status.keep_awake,
                "policy": status.policy,
                "hooks": status.hooks_summary,
                "workers": status.workers_summary,
                "approvals": status.approvals_summary,
                "mail": status.mail_summary,
                "local_app": status.local_app,
                "next": status.next,
            })
        );
        return Ok(());
    }
    println!("heiwa app");
    println!("  command: app runtime status");
    println!("  state: {}", status.state);
    println!("  node: {}", status.node);
    println!("  cli: {}", status.cli_path.display());
    println!("  state_dir: {}", status.state_dir.display());
    println!("  transport: {}", status.transport);
    println!("  sidecar: {}", status.sidecar);
    println!("  keep_awake: {}", status.keep_awake);
    println!("  policy: {}", status.policy);
    println!(
        "  hooks: {} active / {} degraded / {} unconfigured",
        status
            .hooks_summary
            .get("active")
            .and_then(Value::as_i64)
            .unwrap_or(0),
        status
            .hooks_summary
            .get("degraded")
            .and_then(Value::as_i64)
            .unwrap_or(0),
        status
            .hooks_summary
            .get("unconfigured")
            .and_then(Value::as_i64)
            .unwrap_or(0),
    );
    println!(
        "  workers: {} live / {} stale",
        status
            .workers_summary
            .get("live")
            .and_then(Value::as_i64)
            .unwrap_or(0),
        status
            .workers_summary
            .get("stale")
            .and_then(Value::as_i64)
            .unwrap_or(0),
    );
    println!(
        "  approvals: {} pending",
        status
            .approvals_summary
            .get("pending")
            .and_then(Value::as_i64)
            .unwrap_or(0)
    );
    println!(
        "  mail: {} (policy: {})",
        status
            .mail_summary
            .get("bridge_state")
            .and_then(Value::as_str)
            .unwrap_or("unknown"),
        status
            .mail_summary
            .get("policy")
            .and_then(Value::as_str)
            .unwrap_or("metadata-only-no-body"),
    );
    println!(
        "  local_app: {} on {} ({})",
        if status.local_app.reachable {
            "reachable"
        } else {
            "unreachable"
        },
        status.local_app.url,
        status
            .local_app
            .latency_ms
            .map(|ms| format!("{ms}ms"))
            .unwrap_or_else(|| "not running".to_string()),
    );
    println!("  next: {}", status.next);
    Ok(())
}

struct RuntimeStatus {
    state: &'static str,
    node: String,
    cli_path: PathBuf,
    state_dir: PathBuf,
    transport: &'static str,
    sidecar: &'static str,
    keep_awake: String,
    policy: &'static str,
    next: &'static str,
    hooks_summary: Value,
    workers_summary: Value,
    approvals_summary: Value,
    mail_summary: Value,
    local_app: LocalAppProbe,
}

impl RuntimeStatus {
    fn detect() -> Self {
        let state_dir = state_dir();
        Self {
            state: "local_probe",
            node: hostname_string(),
            cli_path: env::current_exe().unwrap_or_else(|_| PathBuf::from("heiwa")),
            state_dir: state_dir.clone(),
            transport: "localhost-http-websocket-ready",
            sidecar: "start-with-heiwa-app-start",
            keep_awake: detect_keep_awake(),
            policy: "local-only-no-side-effects",
            next: "run heiwa app start --port 7474",
            hooks_summary: hooks_summary(),
            workers_summary: workers_summary(&state_dir),
            approvals_summary: approvals_summary(&state_dir),
            mail_summary: mail_summary(),
            local_app: probe_local_app(DEFAULT_PORT),
        }
    }
}

async fn handle_connection(mut stream: TcpStream, started_at: Arc<String>) -> Result<()> {
    let (request, body) = read_http_request_and_body(&mut stream).await?;
    if request.is_empty() {
        return Ok(());
    }

    if is_websocket_request(&request) {
        let path = request_path(&request).unwrap_or("/").to_string();
        return handle_websocket(stream, &request, started_at, &path).await;
    }

    let method = request_method(&request).unwrap_or("GET");
    let path = request_path(&request).unwrap_or("/");
    if method == "OPTIONS" {
        return write_response(&mut stream, 204, "text/plain", Vec::new(), false).await;
    }
    let head_only = method == "HEAD";

    if method == "POST" && path == "/api/v1/repl" {
        let parsed_body: Value = serde_json::from_str(&body).unwrap_or_else(|_| json!({}));
        let prompt = parsed_body
            .get("prompt")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();

        let payload = match crate::execute_repl_turn(&prompt).await {
            Ok((response, trace)) => {
                json!({
                    "ok": true,
                    "data": {
                        "response": response,
                        "trace": trace,
                    }
                })
            }
            Err(err) => {
                json!({
                    "ok": false,
                    "error": {
                        "code": "execution_failed",
                        "message": err,
                    }
                })
            }
        };

        return write_response(
            &mut stream,
            200,
            "application/json",
            payload.to_string().into_bytes(),
            false,
        )
        .await;
    }

    if method != "GET" && !head_only {
        return write_response(
            &mut stream,
            405,
            "application/json",
            json!({"ok": false, "error": {"code": "method_not_allowed"}})
                .to_string()
                .into_bytes(),
            false,
        )
        .await;
    }

    if let Some(payload) = api_payload(path, &started_at) {
        return write_response(
            &mut stream,
            200,
            "application/json",
            payload.to_string().into_bytes(),
            head_only,
        )
        .await;
    }

    serve_static(&mut stream, path, head_only).await
}

async fn read_http_request_and_body(stream: &mut TcpStream) -> Result<(String, String)> {
    let mut data = Vec::new();
    let mut buf = [0u8; 1024];
    let mut headers_len = None;

    loop {
        let n = stream.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        data.extend_from_slice(&buf[..n]);
        if let Some(pos) = data.windows(4).position(|w| w == b"\r\n\r\n") {
            headers_len = Some(pos + 4);
            break;
        }
        if data.len() > 64 * 1024 {
            return Err(anyhow!("request headers too large"));
        }
    }

    let headers_len = match headers_len {
        Some(len) => len,
        None => return Err(anyhow!("missing http headers separator")),
    };

    let headers_str = String::from_utf8_lossy(&data[..headers_len]).to_string();

    let mut content_length = 0;
    if let Some(cl_str) = header_value(&headers_str, "content-length") {
        if let Ok(len) = cl_str.trim().parse::<usize>() {
            content_length = len;
        }
    }

    let total_len = headers_len + content_length;
    while data.len() < total_len {
        let n = stream.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        data.extend_from_slice(&buf[..n]);
        if data.len() > 10 * 1024 * 1024 {
            return Err(anyhow!("request body too large"));
        }
    }

    let body_str = String::from_utf8_lossy(&data[headers_len..total_len]).to_string();
    Ok((headers_str, body_str))
}

async fn handle_websocket(
    mut stream: TcpStream,
    request: &str,
    started_at: Arc<String>,
    path: &str,
) -> Result<()> {
    let key = header_value(request, "sec-websocket-key")
        .ok_or_else(|| anyhow!("missing websocket key"))?;
    let accept = websocket_accept_key(&key);
    let response = format!(
        "HTTP/1.1 101 Switching Protocols\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Accept: {accept}\r\n\
         \r\n"
    );
    stream.write_all(response.as_bytes()).await?;

    if path == "/ws/v1/events" {
        return events_loop(stream).await;
    }

    let mut ticker = time::interval(Duration::from_secs(5));
    loop {
        ticker.tick().await;
        let payload = json!({
            "type": "runtime_snapshot",
            "data": snapshot(&started_at),
        });
        if write_ws_text(&mut stream, &payload.to_string())
            .await
            .is_err()
        {
            break;
        }
    }
    Ok(())
}

async fn events_loop(mut stream: TcpStream) -> Result<()> {
    let mut last_pending: HashSet<String> = HashSet::new();
    let mut last_decided: HashSet<String> = HashSet::new();
    let mut last_goals_fingerprint: HashSet<(String, u64)> = HashSet::new();
    let mut first = true;
    let mut heartbeat_counter: u32 = 0;
    let mut ticker = time::interval(Duration::from_secs(2));

    loop {
        ticker.tick().await;
        let pending = scan_dispatch_ids("requests");
        let decided = scan_dispatch_ids("approvals/decisions");
        let goals_fp = scan_goals_fingerprint();
        let ts = chrono::Utc::now().to_rfc3339();

        if first {
            let payload = json!({
                "event": "events_initial",
                "ts_utc": ts,
                "scope": "approvals",
                "payload": {
                    "pending_count": pending.len(),
                    "decided_count": decided.len(),
                    "goals_count": goals_fp.len(),
                }
            });
            if write_ws_text(&mut stream, &payload.to_string())
                .await
                .is_err()
            {
                return Ok(());
            }
            first = false;
        } else {
            let mut emitted = false;
            for id in pending.difference(&last_pending) {
                let payload = json!({
                    "event": "dispatch_request_appeared",
                    "ts_utc": ts,
                    "scope": "approvals",
                    "payload": { "id": id }
                });
                if write_ws_text(&mut stream, &payload.to_string())
                    .await
                    .is_err()
                {
                    return Ok(());
                }
                emitted = true;
            }
            for id in decided.difference(&last_decided) {
                let payload = json!({
                    "event": "dispatch_request_decided",
                    "ts_utc": ts,
                    "scope": "approvals",
                    "payload": { "id": id }
                });
                if write_ws_text(&mut stream, &payload.to_string())
                    .await
                    .is_err()
                {
                    return Ok(());
                }
                emitted = true;
            }
            if goals_fp != last_goals_fingerprint {
                let payload = json!({
                    "event": "goal_updated",
                    "ts_utc": ts,
                    "scope": "goals",
                    "payload": { "count": goals_fp.len() }
                });
                if write_ws_text(&mut stream, &payload.to_string())
                    .await
                    .is_err()
                {
                    return Ok(());
                }
                emitted = true;
            }
            heartbeat_counter += 1;
            if !emitted && heartbeat_counter >= 15 {
                let payload = json!({ "event": "heartbeat", "ts_utc": ts });
                if write_ws_text(&mut stream, &payload.to_string())
                    .await
                    .is_err()
                {
                    return Ok(());
                }
                heartbeat_counter = 0;
            } else if emitted {
                heartbeat_counter = 0;
            }
        }

        last_pending = pending;
        last_decided = decided;
        last_goals_fingerprint = goals_fp;
    }
}

fn scan_goals_fingerprint() -> HashSet<(String, u64)> {
    let dir = crate::cmd::goal::goals_dir();
    let mut out = HashSet::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let mtime = fs::metadata(&path)
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        out.insert((stem.to_string(), mtime));
    }
    out
}

fn scan_dispatch_ids(subdir: &str) -> HashSet<String> {
    let home = env::var("HOME")
        .map(PathBuf::from)
        .ok()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."));
    let dir = home
        .join(".heiwa")
        .join("state")
        .join("dispatch")
        .join(subdir);
    scan_dispatch_ids_in(&dir)
}

fn scan_dispatch_ids_in(dir: &Path) -> HashSet<String> {
    let mut ids = HashSet::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return ids;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
            ids.insert(stem.to_string());
        }
    }
    ids
}

async fn write_ws_text(stream: &mut TcpStream, text: &str) -> Result<()> {
    let bytes = text.as_bytes();
    let mut frame = Vec::with_capacity(bytes.len() + 10);
    frame.push(0x81);
    match bytes.len() {
        len if len < 126 => frame.push(len as u8),
        len if len <= u16::MAX as usize => {
            frame.push(126);
            frame.extend_from_slice(&(len as u16).to_be_bytes());
        }
        len => {
            frame.push(127);
            frame.extend_from_slice(&(len as u64).to_be_bytes());
        }
    }
    frame.extend_from_slice(bytes);
    stream.write_all(&frame).await?;
    Ok(())
}

async fn serve_static(stream: &mut TcpStream, path: &str, head_only: bool) -> Result<()> {
    let root = cockpit_static_root();
    let file = static_file_for(&root, path);
    let Ok(bytes) = fs::read(&file) else {
        return write_response(
            stream,
            404,
            "application/json",
            json!({"ok": false, "error": {"code": "not_found"}})
                .to_string()
                .into_bytes(),
            head_only,
        )
        .await;
    };
    write_response(stream, 200, content_type(&file), bytes, head_only).await
}

async fn write_response(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    body: Vec<u8>,
    head_only: bool,
) -> Result<()> {
    let status_text = match status {
        200 => "OK",
        204 => "No Content",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "OK",
    };
    let header = format!(
        "HTTP/1.1 {status} {status_text}\r\n\
         Content-Length: {}\r\n\
         Content-Type: {content_type}\r\n\
         Cache-Control: no-store\r\n\
         Access-Control-Allow-Origin: *\r\n\
         Access-Control-Allow-Headers: content-type, authorization\r\n\
         Access-Control-Allow-Methods: GET, POST, OPTIONS\r\n\
         Connection: close\r\n\
         \r\n",
        body.len()
    );
    stream.write_all(header.as_bytes()).await?;
    if !head_only {
        stream.write_all(&body).await?;
    }
    Ok(())
}

fn api_payload(path: &str, started_at: &str) -> Option<Value> {
    let data = match path {
        "/status/health" => json!({
            "status": "ok",
            "runtime_version": env!("CARGO_PKG_VERSION"),
            "started_at": started_at,
            "notes": ["heiwa-shell local app runtime"],
        }),
        "/api/runtime/snapshot" | "/api/v1/runtime/snapshot" => snapshot(started_at),
        "/api/v1/resource" => resource_payload(),
        "/api/v1/session" => json!({
            "operator_id": env::var("USER").unwrap_or_else(|_| "local-operator".to_string()),
            "hostname": hostname_string(),
            "runtime_version": env!("CARGO_PKG_VERSION"),
            "channel": "stable",
            "default_route_role": "local_first",
            "app_url": format!("http://127.0.0.1:{DEFAULT_PORT}/"),
        }),
        "/api/v1/providers" => json!({ "providers": provider_rows() }),
        "/api/v1/routes" => json!({ "routes": route_rows() }),
        "/api/v1/hooks" => json!({ "providers": hook_provider_rows(), "summary": hooks_summary() }),
        "/api/v1/missions" => json!({ "missions": [], "cursor": null }),
        "/api/v1/approvals" => json!({ "approvals": approval_rows() }),
        "/api/v1/approvals/summary" => crate::cmd::approvals::pending_approvals_summary_payload(),
        "/api/v1/life/today" => crate::cmd::life::today_payload(),
        "/api/v1/life/freshness" => crate::cmd::life::freshness_payload(),
        "/api/v1/goals" => crate::cmd::goal::goals_payload(),
        "/api/v1/compress/summary" => crate::cmd::compress::compress_summary_payload(),
        "/api/v1/rate-groups" => json!({ "rate_groups": rate_group_rows() }),
        "/api/v1/inbox" => {
            let state_dir = state_dir();
            json!({ "items": inbox_items_for_state_dir(&state_dir), "cursor": null })
        }
        "/api/v1/history" => {
            let state_dir = state_dir();
            history_summary_for_state_dir(&state_dir)
        }
        "/api/v1/traces" => json!({ "traces": [] }),
        "/api/v1/memory" => json!({ "entries": [] }),
        "/api/v1/agents" => json!({ "agents": worker_agent_rows() }),
        "/api/v1/capabilities" => capabilities_payload(),
        "/api/v1/crons" => json!({ "crons": [] }),
        "/api/v1/cells/catalog" => json!({ "cells": [] }),
        _ => return None,
    };
    Some(json!({ "ok": true, "data": data }))
}

fn snapshot(started_at: &str) -> Value {
    let status = RuntimeStatus::detect();
    json!({
        "runtime": {
            "status": "ok",
            "started_at": started_at,
            "version": env!("CARGO_PKG_VERSION"),
            "node": status.node,
            "transport": status.transport,
            "keep_awake": status.keep_awake,
        },
        "workers": status.workers_summary,
        "approvals": status.approvals_summary,
        "mail": status.mail_summary,
        "hooks": status.hooks_summary,
        "providers": provider_rows(),
        "resource": resource_payload(),
    })
}

fn resource_payload() -> Value {
    let policy = ResourcePolicy::default();
    let (free_memory_bytes, free_memory_source) = free_memory_bytes();
    let (load_1m, load_source) = load_1m();
    let snapshot = ResourceSnapshot {
        cpu_count: std::thread::available_parallelism()
            .map(|count| count.get() as u32)
            .unwrap_or(1),
        load_1m,
        free_memory_bytes,
        battery_percent: None,
        on_battery: false,
        thermal_pressure: ThermalPressure::Unknown,
    };
    let admissions = json!({
        "foreground_interactive": policy.admit(&snapshot, WorkClass::ForegroundInteractive),
        "background_watch": policy.admit(&snapshot, WorkClass::BackgroundWatch),
        "local_summary": policy.admit(&snapshot, WorkClass::LocalSummary),
        "local_model_small": policy.admit(&snapshot, WorkClass::LocalModelSmall),
        "local_model_large": policy.admit(&snapshot, WorkClass::LocalModelLarge),
        "provider_escalation": policy.admit(&snapshot, WorkClass::ProviderEscalation),
    });

    json!({
        "snapshot": snapshot,
        "policy": policy,
        "admissions": admissions,
        "sources": {
            "cpu_count": "std::thread::available_parallelism",
            "load_1m": load_source,
            "free_memory_bytes": free_memory_source,
            "battery_percent": "not_probed_v0",
            "thermal_pressure": "unknown_v0",
        },
        "notes": [
            "read_only_local_probe",
            "resource policy gates local always-on work before provider routing"
        ],
    })
}

fn capabilities_payload() -> Value {
    capabilities_payload_for_state_dir(&state_dir())
}

fn capabilities_payload_for_state_dir(state_dir: &Path) -> Value {
    let capabilities_dir = state_dir.join("capabilities");
    let mut catalogs = Vec::new();
    let Ok(entries) = fs::read_dir(&capabilities_dir) else {
        return json!({
            "catalogs": [],
            "latest": Value::Null,
            "path": capabilities_dir.display().to_string(),
        });
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let Ok(raw) = fs::read_to_string(&path) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<Value>(&raw) else {
            continue;
        };
        let catalog_id = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("capability-catalog")
            .to_string();
        catalogs.push(json!({
            "catalog_id": catalog_id,
            "path": path.display().to_string(),
            "schema_version": value.get("schema_version").and_then(Value::as_str).unwrap_or("unknown"),
            "generated_at": value.get("generated_at").and_then(Value::as_str),
            "providers": value.get("providers").and_then(Value::as_array).map(Vec::len).unwrap_or(0),
            "gemini_extensions": value.get("gemini_extensions").and_then(Value::as_array).map(Vec::len).unwrap_or(0),
            "codex_plugins_observed": value.get("codex_plugins_observed").and_then(Value::as_array).map(Vec::len).unwrap_or(0),
            "codex_mcp_servers": value.get("codex_mcp_servers").and_then(Value::as_array).map(Vec::len).unwrap_or(0),
            "claude_plugins_observed": value.get("claude_plugins_observed").and_then(Value::as_array).map(Vec::len).unwrap_or(0),
            "gemini_skills_observed": value.get("gemini_skills_observed").and_then(Value::as_array).map(Vec::len).unwrap_or(0),
            "installed_apps_observed": value.get("installed_apps_observed").and_then(Value::as_array).map(Vec::len).unwrap_or(0),
            "peer_handoff_findings": value.get("peer_handoff_findings").and_then(Value::as_array).map(Vec::len).unwrap_or(0),
            "reference_sources": value.get("reference_sources").and_then(Value::as_array).map(Vec::len).unwrap_or(0),
            "integration_families": value.get("integration_families").and_then(Value::as_array).map(Vec::len).unwrap_or(0),
            "runtime_targets": value.get("runtime_targets").and_then(Value::as_array).map(Vec::len).unwrap_or(0),
            "performance_targets": value.get("performance_targets").and_then(Value::as_array).map(Vec::len).unwrap_or(0),
            "next_runtime_targets": value.get("next_runtime_targets").cloned().unwrap_or_else(|| json!([])),
        }));
    }

    catalogs.sort_by(|a, b| {
        let a_id = a.get("catalog_id").and_then(Value::as_str).unwrap_or("");
        let b_id = b.get("catalog_id").and_then(Value::as_str).unwrap_or("");
        b_id.cmp(a_id)
    });
    let latest = catalogs.first().cloned().unwrap_or(Value::Null);

    json!({
        "catalogs": catalogs,
        "latest": latest,
        "path": capabilities_dir.display().to_string(),
    })
}

fn load_1m() -> (f32, &'static str) {
    #[cfg(unix)]
    {
        let mut loads = [0.0_f64; 3];
        let count = unsafe { libc::getloadavg(loads.as_mut_ptr(), 1) };
        if count == 1 {
            return (loads[0] as f32, "libc_getloadavg");
        }
    }
    (0.0, "unavailable_default_zero")
}

fn free_memory_bytes() -> (u64, &'static str) {
    if let Some(bytes) = linux_mem_available_bytes() {
        return (bytes, "linux_proc_meminfo_memavailable");
    }
    if let Some(bytes) = macos_memory_pressure_available_bytes() {
        return (bytes, "macos_memory_pressure_free_percentage");
    }
    if let Some(bytes) = macos_vm_stat_available_bytes() {
        return (bytes, "macos_vm_stat_free_inactive_speculative");
    }
    (u64::MAX, "unavailable_assumed_unconstrained")
}

#[cfg(target_os = "linux")]
fn linux_mem_available_bytes() -> Option<u64> {
    let raw = fs::read_to_string("/proc/meminfo").ok()?;
    raw.lines().find_map(|line| {
        let rest = line.strip_prefix("MemAvailable:")?;
        let kb = rest.split_whitespace().next()?.parse::<u64>().ok()?;
        Some(kb * 1024)
    })
}

#[cfg(not(target_os = "linux"))]
fn linux_mem_available_bytes() -> Option<u64> {
    None
}

#[cfg(target_os = "macos")]
fn macos_memory_pressure_available_bytes() -> Option<u64> {
    let output = Command::new("/usr/bin/memory_pressure").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let raw = String::from_utf8_lossy(&output.stdout);
    parse_macos_memory_pressure_available_bytes(&raw)
}

#[cfg(not(target_os = "macos"))]
fn macos_memory_pressure_available_bytes() -> Option<u64> {
    None
}

#[cfg(target_os = "macos")]
fn parse_macos_memory_pressure_available_bytes(raw: &str) -> Option<u64> {
    let total_bytes = raw.lines().find_map(|line| {
        let rest = line.strip_prefix("The system has ")?;
        rest.split_whitespace().next()?.parse::<u64>().ok()
    })?;
    let free_percent = raw.lines().find_map(|line| {
        let rest = line.strip_prefix("System-wide memory free percentage: ")?;
        rest.trim_end_matches('%').trim().parse::<u64>().ok()
    })?;
    Some(total_bytes.saturating_mul(free_percent) / 100)
}

#[cfg(target_os = "macos")]
fn macos_vm_stat_available_bytes() -> Option<u64> {
    let output = Command::new("/usr/bin/vm_stat").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let raw = String::from_utf8_lossy(&output.stdout);
    let page_size = parse_vm_stat_page_size(&raw)?;
    let pages = parse_vm_stat_pages(&raw, "Pages free")
        + parse_vm_stat_pages(&raw, "Pages inactive")
        + parse_vm_stat_pages(&raw, "Pages speculative");
    Some(pages * page_size)
}

#[cfg(not(target_os = "macos"))]
fn macos_vm_stat_available_bytes() -> Option<u64> {
    None
}

#[cfg(target_os = "macos")]
fn parse_vm_stat_page_size(raw: &str) -> Option<u64> {
    let marker = "page size of ";
    let (_, rest) = raw.lines().next()?.split_once(marker)?;
    let bytes = rest.split_whitespace().next()?.parse::<u64>().ok()?;
    Some(bytes)
}

#[cfg(target_os = "macos")]
fn parse_vm_stat_pages(raw: &str, label: &str) -> u64 {
    raw.lines()
        .find_map(|line| {
            let rest = line.trim().strip_prefix(label)?.trim_start_matches(':').trim();
            rest.trim_end_matches('.').replace('.', "").parse::<u64>().ok()
        })
        .unwrap_or(0)
}

fn provider_rows() -> Vec<Value> {
    ["ollama", "gemini", "antigravity", "claude", "codex"]
        .iter()
        .filter_map(|provider| heiwa_provider::get_auth_status(provider))
        .map(|account| {
            json!({
                "provider_id": account.provider_id,
                "display_name": provider_display_name(&account.provider_id),
                "auth_kind": auth_kind_label(&account.auth_kind),
                "status": cockpit_status(&account.status),
                "rate_group": account.rate_group,
                "default_model": account.default_model,
                "last_validated_at": chrono::Utc::now().to_rfc3339(),
                "last_error": if cockpit_status(&account.status) == "connected" { Value::Null } else { Value::String(account.status.clone()) },
                "supported_lanes": supported_lanes(&account.provider_id),
            })
        })
        .collect()
}

fn route_rows() -> Vec<Value> {
    vec![
        json!({
            "role": "chat",
            "provider": "ollama",
            "model": "local-default",
            "source": "default",
            "fallbacks": ["gemini", "claude", "codex"],
            "offline_capable": true,
        }),
        json!({
            "role": "code",
            "provider": "codex",
            "model": "provider-default",
            "source": "default",
            "fallbacks": ["claude", "gemini", "ollama"],
            "offline_capable": false,
        }),
        json!({
            "role": "research",
            "provider": "gemini",
            "model": "provider-default",
            "source": "default",
            "fallbacks": ["codex", "claude"],
            "offline_capable": false,
        }),
    ]
}

fn approval_rows() -> Vec<Value> {
    let requests = state_dir().join("dispatch").join("requests");
    let Ok(entries) = fs::read_dir(requests) else {
        return Vec::new();
    };
    let mut rows = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let raw = fs::read_to_string(&path).unwrap_or_default();
        let value: Value = serde_json::from_str(&raw).unwrap_or_else(|_| json!({}));
        let fallback_id = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("approval")
            .to_string();
        let approval_id = value
            .get("id")
            .or_else(|| value.get("task_id"))
            .and_then(Value::as_str)
            .unwrap_or(&fallback_id)
            .to_string();
        let summary = value
            .get("summary")
            .or_else(|| value.get("action"))
            .or_else(|| value.get("reason"))
            .and_then(Value::as_str)
            .unwrap_or("Local approval request")
            .to_string();
        rows.push(json!({
            "approval_id": approval_id,
            "mission_id": value.get("mission_id").or_else(|| value.get("task_id")).and_then(Value::as_str).unwrap_or("local-dispatch"),
            "risk_level": value.get("risk_level").or_else(|| value.get("risk")).or_else(|| value.get("risk_tier")).and_then(Value::as_str).unwrap_or("unknown"),
            "summary": summary,
            "requested_at": value.get("requested_at").or_else(|| value.get("created_at")).and_then(Value::as_str).unwrap_or("unknown"),
            "expires_at": Value::Null,
            "requested_by": value.get("requested_by").or_else(|| value.get("from")).and_then(Value::as_str).unwrap_or("local-dispatch"),
        }));
    }
    rows
}

fn history_summary_for_state_dir(state_dir: &Path) -> Value {
    let dispatch_results = dispatch_result_values_for_state_dir(state_dir);
    let mut recent_runs = dispatch_results
        .iter()
        .map(|(_, value)| {
            let request_id = string_field(value, &["request_id", "mission_id", "task_id"])
                .unwrap_or("local-dispatch");
            json!({
                "mission_id": request_id,
                "status": string_field(value, &["status"]).unwrap_or("unknown"),
                "updated_at": string_field(value, &["completed_at", "updated_at", "created_at"]).unwrap_or("unknown"),
                "summary": string_field(value, &["summary"]),
            })
        })
        .collect::<Vec<_>>();
    sort_values_by_time_desc(&mut recent_runs, "updated_at");
    recent_runs.truncate(40);

    let mut artifacts = Vec::new();
    for (_, result) in &dispatch_results {
        let updated_at = string_field(result, &["completed_at", "updated_at", "created_at"])
            .unwrap_or("unknown");
        if let Some(refs) = result.get("evidence_refs").and_then(Value::as_array) {
            for evidence_ref in refs.iter().filter_map(Value::as_str) {
                artifacts.push(json!({
                    "id": evidence_ref,
                    "kind": "evidence_ref",
                    "label": evidence_ref,
                    "updated_at": updated_at,
                }));
            }
        }
    }
    sort_values_by_time_desc(&mut artifacts, "updated_at");
    artifacts.truncate(80);

    json!({
        "sessions": [],
        "recent_runs": recent_runs,
        "artifacts": artifacts,
        "cursor": null,
    })
}

fn inbox_items_for_state_dir(state_dir: &Path) -> Vec<Value> {
    let mut items = Vec::new();
    items.extend(event_log_items_for_state_dir(state_dir));
    items.extend(
        dispatch_result_values_for_state_dir(state_dir)
            .into_iter()
            .map(|(path, value)| dispatch_result_inbox_item(&path, &value)),
    );
    sort_values_by_time_desc(&mut items, "occurred_at");
    items.truncate(80);
    items
}

fn dispatch_result_inbox_item(path: &Path, result: &Value) -> Value {
    let result_id = string_field(result, &["result_id"])
        .or_else(|| path.file_stem().and_then(|stem| stem.to_str()))
        .unwrap_or("dispatch-result");
    let request_id =
        string_field(result, &["request_id", "mission_id", "task_id"]).unwrap_or("local-dispatch");
    let occurred_at =
        string_field(result, &["completed_at", "updated_at", "created_at"]).unwrap_or("unknown");
    let adapter = string_field(result, &["adapter"]).unwrap_or("dispatch");
    let status = string_field(result, &["status"]).unwrap_or("unknown");
    let summary = string_field(result, &["summary"]).unwrap_or("Dispatch result recorded");
    let receipt_refs = result
        .get("evidence_refs")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|evidence_ref| evidence_ref.as_str().map(str::to_string))
        .map(|evidence_ref| {
            json!({
                "kind": "evidence_ref",
                "ref": evidence_ref,
            })
        })
        .collect::<Vec<_>>();

    json!({
        "item_id": format!("receipt:{result_id}"),
        "kind": "dispatch_result",
        "plane": "evidence",
        "priority": priority_for_status(status),
        "pinned": false,
        "status": status,
        "title": format!("{adapter} {status}"),
        "summary": summary,
        "occurred_at": occurred_at,
        "source": source_ref(result_id, "dispatch_result", adapter, path),
        "subject_ref": request_id,
        "receipt_refs": receipt_refs,
    })
}

fn event_log_items_for_state_dir(state_dir: &Path) -> Vec<Value> {
    let events_path = state_dir.join("events").join("events.jsonl");
    let Ok(raw) = fs::read_to_string(&events_path) else {
        return Vec::new();
    };
    raw.lines()
        .rev()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .take(40)
        .map(|event| event_log_inbox_item(&events_path, &event))
        .collect()
}

fn event_log_inbox_item(path: &Path, event: &Value) -> Value {
    let event_id = string_field(event, &["event_id"]).unwrap_or("event");
    let event_type = string_field(event, &["event_type"]).unwrap_or("event");
    let source = string_field(event, &["source"]).unwrap_or("local-state");
    let subject = string_field(event, &["subject"]).unwrap_or(event_type);
    let occurred_at = string_field(event, &["ts", "created_at", "updated_at"]).unwrap_or("unknown");
    let payload_ref = string_field(event, &["payload_ref"]);
    let receipt_refs = payload_ref
        .map(|payload_ref| {
            vec![json!({
                "kind": "payload_ref",
                "ref": payload_ref,
            })]
        })
        .unwrap_or_default();

    json!({
        "item_id": format!("event:{event_id}"),
        "kind": "event",
        "plane": plane_for_event_type(event_type),
        "priority": priority_for_severity(string_field(event, &["severity"]).unwrap_or("info")),
        "pinned": false,
        "status": string_field(event, &["severity"]).unwrap_or("info"),
        "title": event_type,
        "summary": subject,
        "occurred_at": occurred_at,
        "source": source_ref(event_id, "event_log", source, path),
        "subject_ref": subject,
        "receipt_refs": receipt_refs,
    })
}

fn dispatch_result_values_for_state_dir(state_dir: &Path) -> Vec<(PathBuf, Value)> {
    let results = state_dir.join("dispatch").join("results");
    let Ok(entries) = fs::read_dir(results) else {
        return Vec::new();
    };
    let mut values = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                return None;
            }
            let value = fs::read_to_string(&path)
                .ok()
                .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())?;
            Some((path, value))
        })
        .collect::<Vec<_>>();
    values.sort_by(|(_, a), (_, b)| {
        string_field(b, &["completed_at", "updated_at", "created_at"])
            .unwrap_or("")
            .cmp(string_field(a, &["completed_at", "updated_at", "created_at"]).unwrap_or(""))
    });
    values.truncate(80);
    values
}

fn source_ref(source_id: &str, source_type: &str, label: &str, path: &Path) -> Value {
    json!({
        "source_id": source_id,
        "source_type": source_type,
        "label": label,
        "uri": path.display().to_string(),
    })
}

fn string_field<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_str))
}

fn sort_values_by_time_desc(values: &mut [Value], field: &str) {
    values.sort_by(|a, b| {
        b.get(field)
            .and_then(Value::as_str)
            .unwrap_or("")
            .cmp(a.get(field).and_then(Value::as_str).unwrap_or(""))
    });
}

fn plane_for_event_type(event_type: &str) -> &'static str {
    if event_type.contains("result") || event_type.contains("evidence") {
        "evidence"
    } else if event_type.contains("request.created")
        || event_type.contains("message")
        || event_type.contains("mail")
        || event_type.contains("calendar")
        || event_type.contains("forum")
    {
        "intake"
    } else if event_type.contains("policy")
        || event_type.contains("worker")
        || event_type.contains("doctor")
        || event_type.contains("dispatch.")
    {
        "execution"
    } else {
        "intake"
    }
}

fn priority_for_status(status: &str) -> &'static str {
    match status {
        "failed" | "denied" | "error" => "high",
        "pending" | "running" => "normal",
        _ => "low",
    }
}

fn priority_for_severity(severity: &str) -> &'static str {
    match severity {
        "error" => "high",
        "warn" => "normal",
        _ => "low",
    }
}

fn rate_group_rows() -> Vec<Value> {
    let providers = provider_rows();
    let groups = [
        ("local", 1),
        ("google", 2),
        ("google_bonus", 3),
        ("anthropic", 4),
        ("openai", 5),
    ];
    groups
        .iter()
        .map(|(group, priority)| {
            let members = providers
                .iter()
                .filter(|provider| {
                    provider
                        .get("rate_group")
                        .and_then(Value::as_str)
                        .is_some_and(|rate_group| rate_group == *group)
                })
                .filter_map(|provider| provider.get("provider_id").and_then(Value::as_str))
                .collect::<Vec<_>>();
            let healthy = providers.iter().any(|provider| {
                provider
                    .get("rate_group")
                    .and_then(Value::as_str)
                    .is_some_and(|rate_group| rate_group == *group)
                    && provider
                        .get("status")
                        .and_then(Value::as_str)
                        .is_some_and(|status| status == "connected")
            });
            json!({
                "group_id": group,
                "priority": priority,
                "status": if healthy { "healthy" } else { "down" },
                "providers": members,
                "quota_state": {},
                "notes": "local runtime discovery",
            })
        })
        .collect()
}

fn worker_agent_rows() -> Vec<Value> {
    let workers = fs::read_to_string(state_dir().join("workers.json"))
        .ok()
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        .unwrap_or_else(|| json!({"workers": []}));
    workers
        .get("workers")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|worker| {
            let id = worker
                .get("worker_id")
                .and_then(Value::as_str)
                .unwrap_or("local-worker");
            json!({
                "agent_id": id,
                "parent_id": Value::Null,
                "status": "running",
                "role": worker.get("class").and_then(Value::as_str).unwrap_or("shell_machine"),
                "started_at": worker.get("last_heartbeat_utc").and_then(Value::as_str).unwrap_or("unknown"),
                "last_event_at": worker.get("last_heartbeat_utc").and_then(Value::as_str),
            })
        })
        .collect()
}

fn hook_provider_rows() -> Vec<Value> {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    vec![
        json_hook_provider_row(
            "claude",
            "Claude Code",
            &home.join(".claude").join("settings.json"),
            Some(
                home.join(".heiwa")
                    .join("generated")
                    .join("claude")
                    .join("settings.json"),
            ),
            &["PreToolUse", "UserPromptSubmit"],
            Some(
                home.join(".heiwa")
                    .join("logs")
                    .join("policy")
                    .join("claude-runtime-safety.jsonl"),
            ),
            vec![
                "provider-owned-hook-api",
                "schema-requires-hookEventName",
                "heiwa-observes-and-hardens",
            ],
        ),
        json_hook_provider_row(
            "gemini",
            "Gemini CLI",
            &home.join(".gemini").join("settings.json"),
            Some(
                home.join(".heiwa")
                    .join("generated")
                    .join("gemini")
                    .join("settings.json"),
            ),
            &["BeforeTool", "SessionStart"],
            Some(
                home.join(".heiwa")
                    .join("logs")
                    .join("policy")
                    .join("gemini-runtime-policy.jsonl"),
            ),
            vec![
                "provider-owned-hook-api",
                "before-tool-policy",
                "session-bootstrap",
            ],
        ),
        codex_hook_provider_row(&home),
        json!({
            "provider_id": "antigravity",
            "display_name": "Antigravity",
            "status": "delegated",
            "config_path": home.join(".gemini").join("antigravity").display().to_string(),
            "generated_config_status": generated_file_status(&home.join(".heiwa").join("generated").join("antigravity").join("settings.json")),
            "audit_file": Value::Null,
            "events": [],
            "notes": [
                "inherits-gemini-posture",
                "separate-live-hook-registry-not-detected",
            ],
        }),
    ]
}

fn json_hook_provider_row(
    provider_id: &str,
    display_name: &str,
    config_path: &Path,
    generated_config_path: Option<PathBuf>,
    event_names: &[&str],
    audit_file: Option<PathBuf>,
    notes: Vec<&str>,
) -> Value {
    let events = hook_events_from_json_config(config_path, event_names);
    let command_count = events
        .iter()
        .filter_map(|event| event.get("hooks").and_then(Value::as_array))
        .map(Vec::len)
        .sum::<usize>();
    let missing_command = events
        .iter()
        .filter_map(|event| event.get("hooks").and_then(Value::as_array))
        .flatten()
        .any(|hook| {
            hook.get("command_exists")
                .and_then(Value::as_bool)
                .is_some_and(|exists| !exists)
        });
    let status = if !config_path.exists() {
        "unconfigured"
    } else if command_count == 0 || missing_command {
        "degraded"
    } else {
        "active"
    };
    let generated_config_status = generated_config_path
        .as_deref()
        .map(|path| generated_hook_status(config_path, path))
        .unwrap_or_else(|| "not_applicable".to_string());

    json!({
        "provider_id": provider_id,
        "display_name": display_name,
        "status": status,
        "config_path": config_path.display().to_string(),
        "generated_config_status": generated_config_status,
        "audit_file": audit_file.map(|path| Value::String(path.display().to_string())).unwrap_or(Value::Null),
        "events": events,
        "notes": notes,
    })
}

fn codex_hook_provider_row(home: &Path) -> Value {
    let config_path = home.join(".codex").join("config.toml");
    json!({
        "provider_id": "codex",
        "display_name": "Codex",
        "status": "unsupported",
        "config_path": config_path.display().to_string(),
        "generated_config_status": generated_file_status(&home.join(".heiwa").join("generated").join("codex").join("config.toml")),
        "audit_file": Value::Null,
        "events": [],
        "notes": [
            "native-hook-parity-not-detected",
            "phase-1-safety-launcher-only",
            "app-should-show-boundary-not-fake-parity",
        ],
    })
}

fn hooks_summary() -> Value {
    let rows = hook_provider_rows();
    let mut active = 0i64;
    let mut degraded = 0i64;
    let mut unconfigured = 0i64;
    let mut unsupported = 0i64;
    let mut delegated = 0i64;
    let mut event_count = 0i64;
    let mut command_count = 0i64;

    for row in &rows {
        match row
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
        {
            "active" => active += 1,
            "degraded" => degraded += 1,
            "unconfigured" => unconfigured += 1,
            "unsupported" => unsupported += 1,
            "delegated" => delegated += 1,
            _ => {}
        }
        if let Some(events) = row.get("events").and_then(Value::as_array) {
            event_count += events.len() as i64;
            command_count += events
                .iter()
                .filter_map(|event| event.get("hooks").and_then(Value::as_array))
                .map(|hooks| hooks.len() as i64)
                .sum::<i64>();
        }
    }

    json!({
        "source": "live-home-config",
        "providers": rows.len(),
        "active": active,
        "degraded": degraded,
        "unconfigured": unconfigured,
        "unsupported": unsupported,
        "delegated": delegated,
        "events": event_count,
        "commands": command_count,
    })
}

fn hook_events_from_json_config(config_path: &Path, event_names: &[&str]) -> Vec<Value> {
    let Some(config) = fs::read_to_string(config_path)
        .ok()
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
    else {
        return Vec::new();
    };
    let Some(hooks) = config.get("hooks").and_then(Value::as_object) else {
        return Vec::new();
    };

    let mut events = Vec::new();
    for event_name in event_names {
        let Some(entries) = hooks.get(*event_name).and_then(Value::as_array) else {
            continue;
        };
        for entry in entries {
            let matcher = entry
                .get("matcher")
                .and_then(Value::as_str)
                .unwrap_or("*")
                .to_string();
            let hook_commands = entry
                .get("hooks")
                .and_then(Value::as_array)
                .map(|hooks| {
                    hooks
                        .iter()
                        .map(|hook| {
                            let command = hook
                                .get("command")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_string();
                            let command_path = hook_command_path(&command);
                            let command_exists =
                                command_path.as_deref().map(Path::new).map(Path::exists);
                            json!({
                                "name": hook.get("name").and_then(Value::as_str),
                                "kind": hook.get("type").and_then(Value::as_str),
                                "command": command,
                                "command_path": command_path,
                                "command_exists": command_exists,
                                "timeout_ms": hook.get("timeout").and_then(Value::as_i64),
                            })
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            events.push(json!({
                "event": event_name,
                "matcher": matcher,
                "hooks": hook_commands,
            }));
        }
    }
    events
}

fn hook_command_path(command: &str) -> Option<String> {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    command
        .split_whitespace()
        .rev()
        .map(|token| token.trim_matches('"').trim_matches('\''))
        .find(|token| token.starts_with('/') || token.starts_with("~/"))
        .map(|token| {
            if let Some(rest) = token.strip_prefix("~/") {
                home.join(rest).display().to_string()
            } else {
                token.to_string()
            }
        })
}

fn generated_hook_status(live_path: &Path, generated_path: &Path) -> String {
    match (
        json_hook_fingerprint(live_path),
        json_hook_fingerprint(generated_path),
    ) {
        (Some(live), Some(generated)) if live == generated => "matches_hooks".to_string(),
        (Some(_), Some(_)) => "drift".to_string(),
        (Some(_), None) if generated_path.exists() => "unreadable_generated_hooks".to_string(),
        (Some(_), None) => "no_generated_config".to_string(),
        _ => generated_file_status(generated_path),
    }
}

fn json_hook_fingerprint(path: &Path) -> Option<String> {
    let parsed = fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())?;
    serde_json::to_string(parsed.get("hooks")?).ok()
}

fn generated_file_status(path: &Path) -> String {
    if path.exists() {
        "present-not-live-source".to_string()
    } else {
        "missing".to_string()
    }
}

fn write_app_heartbeat(worker_id: &str) -> Result<()> {
    let path = state_dir().join("workers.json");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut workers = fs::read_to_string(&path)
        .ok()
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        .unwrap_or_else(|| json!({"workers": []}));
    let entry = json!({
        "worker_id": worker_id,
        "class": "shell_machine",
        "node": hostname_string(),
        "last_heartbeat_utc": chrono::Utc::now().to_rfc3339(),
        "ttl_seconds": HEARTBEAT_TTL_SECS,
        "transport": "localhost-http-websocket",
    });
    let arr = workers.as_object_mut().and_then(|obj| {
        obj.entry("workers")
            .or_insert(Value::Array(Vec::new()))
            .as_array_mut()
    });
    if let Some(arr) = arr {
        if let Some(idx) = arr
            .iter()
            .position(|worker| worker.get("worker_id").and_then(Value::as_str) == Some(worker_id))
        {
            arr[idx] = entry;
        } else {
            arr.push(entry);
        }
    }
    fs::write(path, serde_json::to_string_pretty(&workers)?)?;
    Ok(())
}

fn detect_keep_awake() -> String {
    match which("caffeinate") {
        Some(path) => format!(
            "caffeinate-available:{}:used-while-heiwa-app-open",
            path.display()
        ),
        None => "caffeinate-not-found".to_string(),
    }
}

fn spawn_caffeinate() -> Option<Child> {
    let path = which("caffeinate")?;
    Command::new(path)
        .args(["-dimsu"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .ok()
}

fn stop_caffeinate(child: &mut Option<Child>) {
    if let Some(child) = child.as_mut() {
        let _ = child.kill();
        let _ = child.wait();
    }
}

fn open_url(url: &str) -> Result<()> {
    Command::new("/usr/bin/open")
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    Ok(())
}

fn which(bin: &str) -> Option<PathBuf> {
    let output = Command::new("/usr/bin/which").arg(bin).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8(output.stdout).ok()?;
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(PathBuf::from(trimmed))
    }
}

fn workers_summary(state_dir: &Path) -> Value {
    let workers_path = state_dir.join("workers.json");
    let raw = fs::read_to_string(&workers_path).ok();
    let parsed: Value = raw
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_else(|| json!({"workers": []}));
    let entries = parsed
        .get("workers")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let now = chrono::Utc::now().timestamp();
    let mut live = 0i64;
    let mut stale = 0i64;
    for entry in &entries {
        let last = entry
            .get("last_heartbeat_utc")
            .and_then(Value::as_str)
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.timestamp())
            .unwrap_or(0);
        let ttl = entry
            .get("ttl_seconds")
            .and_then(Value::as_i64)
            .unwrap_or(HEARTBEAT_TTL_SECS);
        if (now - last) <= ttl {
            live += 1;
        } else {
            stale += 1;
        }
    }
    json!({
        "path": workers_path.display().to_string(),
        "live": live,
        "stale": stale,
        "total": entries.len(),
    })
}

fn approvals_summary(state_dir: &Path) -> Value {
    let requests = state_dir.join("dispatch").join("requests");
    let decisions = state_dir
        .join("dispatch")
        .join("approvals")
        .join("decisions");
    let pending = count_json(&requests);
    let decided = count_json(&decisions);
    json!({
        "requests_dir": requests.display().to_string(),
        "decisions_dir": decisions.display().to_string(),
        "pending": pending,
        "decided": decided,
    })
}

fn count_json(dir: &Path) -> i64 {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    let mut count = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("json") {
            count += 1;
        }
    }
    count
}

fn mail_summary() -> Value {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let data_dir = home.join("Library").join("Mail");
    let data_present = data_dir.exists();
    json!({
        "policy": "metadata-only-no-body",
        "data_dir": data_dir.display().to_string(),
        "data_present": data_present,
        "bridge_state": if data_present { "ready-for-metadata-probe" } else { "no-mail-data" },
    })
}

fn cockpit_static_root() -> PathBuf {
    let shell_manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo_root = shell_manifest
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let cockpit_dist = repo_root
        .join("apps")
        .join("heiwa_app")
        .join("clients")
        .join("cockpit")
        .join("dist");
    if cockpit_dist.exists() {
        return cockpit_dist;
    }
    repo_root
        .join("apps")
        .join("heiwa_app")
        .join("clients")
        .join("web")
}

fn static_file_for(root: &Path, request_path: &str) -> PathBuf {
    let clean_path = request_path
        .trim_start_matches('/')
        .split('?')
        .next()
        .unwrap_or("");
    let safe = clean_path
        .split('/')
        .filter(|segment| !segment.is_empty())
        .all(|segment| {
            !Path::new(segment)
                .components()
                .any(|component| matches!(component, Component::ParentDir | Component::RootDir))
        });
    if !safe || clean_path.is_empty() {
        return root.join("index.html");
    }
    let candidate = root.join(clean_path);
    if candidate.is_file() {
        return candidate;
    }
    if candidate.is_dir() {
        return candidate.join("index.html");
    }
    let html_candidate = root.join(format!("{clean_path}.html"));
    if html_candidate.is_file() {
        return html_candidate;
    }
    root.join("index.html")
}

fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|ext| ext.to_str()).unwrap_or("") {
        "css" => "text/css; charset=utf-8",
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" => "application/json",
        "png" => "image/png",
        "svg" => "image/svg+xml",
        "wasm" => "application/wasm",
        _ => "application/octet-stream",
    }
}

fn request_method(request: &str) -> Option<&str> {
    request.lines().next()?.split_whitespace().next()
}

fn request_path(request: &str) -> Option<&str> {
    request
        .lines()
        .next()?
        .split_whitespace()
        .nth(1)?
        .split('?')
        .next()
}

fn is_websocket_request(request: &str) -> bool {
    request
        .lines()
        .any(|line| line.to_ascii_lowercase().starts_with("upgrade: websocket"))
}

fn header_value(request: &str, name: &str) -> Option<String> {
    let needle = format!("{name}:");
    request.lines().find_map(|line| {
        if line.to_ascii_lowercase().starts_with(&needle) {
            line.split_once(':')
                .map(|(_, value)| value.trim().to_string())
        } else {
            None
        }
    })
}

fn websocket_accept_key(key: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(key.as_bytes());
    hasher.update(WS_GUID.as_bytes());
    BASE64.encode(hasher.finalize())
}

fn parse_port(args: &[String]) -> Result<u16> {
    match flag_value(args, "--port") {
        Some(raw) => raw
            .parse::<u16>()
            .map_err(|_| anyhow!("invalid --port value: {raw}")),
        None => Ok(DEFAULT_PORT),
    }
}

fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|arg| arg == flag)
}

fn flag_value(args: &[String], flag: &str) -> Option<String> {
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == flag {
            return iter.next().cloned();
        }
        if let Some(rest) = arg.strip_prefix(&format!("{flag}=")) {
            return Some(rest.to_string());
        }
    }
    None
}

fn state_dir() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".heiwa").join("state")
}

fn hostname_string() -> String {
    hostname::get()
        .ok()
        .and_then(|name| name.into_string().ok())
        .unwrap_or_else(|| "unknown-host".to_string())
}

fn provider_display_name(provider: &str) -> &'static str {
    match provider {
        "ollama" => "Ollama",
        "gemini" => "Gemini CLI",
        "antigravity" => "Antigravity",
        "claude" => "Claude Code",
        "codex" => "Codex",
        _ => "Provider",
    }
}

fn auth_kind_label(kind: &heiwa_provider::AuthKind) -> &'static str {
    match kind {
        heiwa_provider::AuthKind::OauthCli => "oauth_cli",
        heiwa_provider::AuthKind::ApiKey => "api_key",
        heiwa_provider::AuthKind::RouterApi => "api_key",
        heiwa_provider::AuthKind::LocalRuntime => "local",
        heiwa_provider::AuthKind::CustomProfile => "subscription",
    }
}

fn cockpit_status(status: &str) -> &'static str {
    match status {
        "connected" | "running" => "connected",
        "installed_unverified" | "installed_stopped" => "degraded",
        "not_installed" => "unlinked",
        _ => "error",
    }
}

fn supported_lanes(provider: &str) -> Vec<&'static str> {
    match provider {
        "ollama" => vec!["local"],
        "claude" | "codex" | "gemini" | "antigravity" => vec!["oauth_cli"],
        _ => vec![],
    }
}

fn print_help() {
    println!("heiwa app");
    println!();
    println!("Usage:");
    println!("  heiwa app start [--port N] [--no-open]");
    println!("  heiwa app update [--source github|checkout] [--dry-run]");
    println!("  heiwa app runtime status [--json]");
    println!("  heiwa app status [--json]");
    println!("  heiwa app [--json]");
    println!();
    println!("Starts or probes the local Heiwa.app cockpit runtime.");
}

fn print_update_help() {
    println!("heiwa app update");
    println!();
    println!("Usage:");
    println!("  heiwa app update [--source github|checkout] [--dry-run]");
    println!();
    println!("Defaults to GitHub Releases for user/runtime updates.");
    println!(
        "Use --source checkout only for explicit developer reinstall from the current checkout."
    );
}

fn print_start_help() {
    println!("heiwa app start");
    println!();
    println!("Usage:");
    println!("  heiwa app start [--port N] [--no-open]");
    println!();
    println!("Binds 127.0.0.1, serves the per-user browser console by default,");
    println!("starts caffeinate while running, and writes a worker heartbeat.");
}

#[cfg(test)]
mod app_readmodel_tests {
    use super::*;

    fn temp_state_dir(name: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!("heiwa-shell-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp state dir");
        dir
    }

    #[test]
    fn api_payload_exposes_life_today_for_cockpit() {
        let payload =
            api_payload("/api/v1/life/today", "2026-05-26T00:00:00Z").expect("life today endpoint");
        assert_eq!(payload.get("ok").and_then(Value::as_bool), Some(true));
        let data = payload.get("data").expect("data envelope");
        assert_eq!(
            data.get("command").and_then(Value::as_str),
            Some("life today")
        );
        assert_eq!(
            data.get("timezone").and_then(Value::as_str),
            Some("America/Vancouver")
        );
        assert!(data.get("pending_approvals").is_some_and(Value::is_array));
        assert!(data
            .get("runtime")
            .and_then(|runtime| runtime.get("stdb_mode"))
            .is_some_and(Value::is_string));
    }

    #[test]
    fn api_payload_exposes_life_freshness_for_cockpit() {
        let payload = api_payload("/api/v1/life/freshness", "2026-05-26T00:00:00Z")
            .expect("life freshness endpoint");
        assert_eq!(payload.get("ok").and_then(Value::as_bool), Some(true));
        let data = payload.get("data").expect("data envelope");
        assert_eq!(
            data.get("command").and_then(Value::as_str),
            Some("life freshness")
        );
        assert!(data.get("stale_sources").is_some_and(Value::is_number));
        assert!(data.get("sources").is_some_and(Value::is_array));
    }

    #[test]
    fn api_payload_exposes_approvals_summary_for_cockpit() {
        let payload = api_payload("/api/v1/approvals/summary", "2026-05-26T00:00:00Z")
            .expect("approvals summary endpoint");
        assert_eq!(payload.get("ok").and_then(Value::as_bool), Some(true));
        let data = payload.get("data").expect("data envelope");
        assert!(data.get("pending_count").is_some_and(Value::is_number));
        assert!(data.get("pending").is_some_and(Value::is_array));
        assert!(data.get("requests_dir").is_some_and(Value::is_string));
    }

    #[test]
    fn dispatch_results_populate_history_runs_and_artifacts() {
        let state = temp_state_dir("history-readmodel");
        let results = state.join("dispatch").join("results");
        fs::create_dir_all(&results).expect("create results dir");
        fs::write(
            results.join("res_demo.json"),
            json!({
                "schema_version": "operator_dispatch_result_v1",
                "request_id": "req_demo",
                "result_id": "res_demo",
                "completed_at": "2026-05-24T12:00:00Z",
                "status": "denied",
                "executed_mode": "none",
                "adapter": "filesystem",
                "summary": "Denied unsafe filesystem write",
                "evidence_refs": ["evidence/2026-05-24/receipt.json"],
                "redaction_applied": true
            })
            .to_string(),
        )
        .expect("write dispatch result");

        let history = history_summary_for_state_dir(&state);
        let runs = history
            .get("recent_runs")
            .and_then(Value::as_array)
            .expect("recent_runs array");
        assert_eq!(runs.len(), 1);
        assert_eq!(
            runs[0].get("mission_id").and_then(Value::as_str),
            Some("req_demo")
        );
        assert_eq!(
            runs[0].get("status").and_then(Value::as_str),
            Some("denied")
        );
        assert_eq!(
            runs[0].get("summary").and_then(Value::as_str),
            Some("Denied unsafe filesystem write")
        );
        let artifacts = history
            .get("artifacts")
            .and_then(Value::as_array)
            .expect("artifacts array");
        assert_eq!(artifacts.len(), 1);
        assert_eq!(
            artifacts[0].get("id").and_then(Value::as_str),
            Some("evidence/2026-05-24/receipt.json")
        );

        let _ = fs::remove_dir_all(&state);
    }

    #[test]
    fn dispatch_results_and_events_populate_inbox_items_with_sources() {
        let state = temp_state_dir("inbox-readmodel");
        let results = state.join("dispatch").join("results");
        let events = state.join("events");
        fs::create_dir_all(&results).expect("create results dir");
        fs::create_dir_all(&events).expect("create events dir");
        fs::write(
            results.join("res_demo.json"),
            json!({
                "schema_version": "operator_dispatch_result_v1",
                "request_id": "req_demo",
                "result_id": "res_demo",
                "completed_at": "2026-05-24T12:00:00Z",
                "status": "denied",
                "executed_mode": "none",
                "adapter": "network",
                "summary": "Blocked external network request",
                "evidence_refs": ["evidence/2026-05-24/network.json"],
                "redaction_applied": true
            })
            .to_string(),
        )
        .expect("write dispatch result");
        fs::write(
            events.join("events.jsonl"),
            format!(
                "{}\n",
                json!({
                    "schema_version": "operator_event_envelope_v1",
                    "event_id": "evt_demo",
                    "ts": "2026-05-24T12:01:00Z",
                    "event_type": "dispatch.policy.classified",
                    "severity": "warn",
                    "source": "devonx",
                    "subject": "network request",
                    "redaction_applied": true,
                    "payload_ref": "dispatch/results/res_demo.json"
                })
            ),
        )
        .expect("write events log");

        let inbox = inbox_items_for_state_dir(&state);
        assert_eq!(inbox.len(), 2);
        assert_eq!(
            inbox[0]
                .get("source")
                .and_then(|s| s.get("source_type"))
                .and_then(Value::as_str),
            Some("event_log")
        );
        assert_eq!(
            inbox[0].get("plane").and_then(Value::as_str),
            Some("execution")
        );
        assert_eq!(
            inbox[1]
                .get("source")
                .and_then(|s| s.get("source_type"))
                .and_then(Value::as_str),
            Some("dispatch_result")
        );
        assert_eq!(
            inbox[1].get("plane").and_then(Value::as_str),
            Some("evidence")
        );
        assert_eq!(
            inbox[1]
                .get("receipt_refs")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(1)
        );

        let _ = fs::remove_dir_all(&state);
    }

    #[test]
    fn event_type_mapping_preserves_iee_flow_planes() {
        assert_eq!(plane_for_event_type("dispatch.request.created"), "intake");
        assert_eq!(
            plane_for_event_type("dispatch.policy.classified"),
            "execution"
        );
        assert_eq!(plane_for_event_type("dispatch.result.written"), "evidence");
    }

    #[test]
    fn scan_dispatch_ids_in_returns_json_file_stems() {
        let dir = temp_state_dir("dispatch-scan");
        fs::write(dir.join("req_alpha.json"), "{}").expect("write alpha");
        fs::write(dir.join("req_beta.json"), "{}").expect("write beta");
        fs::write(dir.join("ignore.txt"), "noop").expect("write decoy");

        let ids = scan_dispatch_ids_in(&dir);
        assert_eq!(ids.len(), 2);
        assert!(ids.contains("req_alpha"));
        assert!(ids.contains("req_beta"));
        assert!(!ids.contains("ignore"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_dispatch_ids_in_returns_empty_on_missing_dir() {
        let missing = env::temp_dir().join("heiwa-shell-dispatch-missing-{nope}");
        let ids = scan_dispatch_ids_in(&missing);
        assert!(ids.is_empty());
    }

    #[test]
    fn api_payload_exposes_goals_for_cockpit() {
        let payload = api_payload("/api/v1/goals", "2026-05-26T00:00:00Z").expect("goals endpoint");
        assert_eq!(payload.get("ok").and_then(Value::as_bool), Some(true));
        let data = payload.get("data").expect("data envelope");
        assert!(data.get("goals_dir").is_some_and(Value::is_string));
        assert!(data.get("goals").is_some_and(Value::is_array));
        assert!(data
            .get("counts")
            .and_then(|c| c.get("open"))
            .is_some_and(Value::is_number));
    }

    #[test]
    fn api_payload_exposes_compress_summary_for_cockpit() {
        let payload = api_payload("/api/v1/compress/summary", "2026-05-26T00:00:00Z")
            .expect("compress summary endpoint");
        assert_eq!(payload.get("ok").and_then(Value::as_bool), Some(true));
        let data = payload.get("data").expect("data envelope");
        assert!(data.get("receipts_dir").is_some_and(Value::is_string));
        assert!(data.get("count").is_some_and(Value::is_number));
        assert!(data
            .get("totals")
            .and_then(|t| t.get("cumulative_ratio"))
            .is_some_and(Value::is_number));
        assert!(data.get("recent").is_some_and(Value::is_array));
    }

    #[test]
    fn resource_api_payload_reports_snapshot_policy_and_admissions() {
        let payload = api_payload("/api/v1/resource", "2026-06-02T00:00:00Z")
            .expect("resource endpoint");
        let data = payload.get("data").expect("data");

        assert!(
            data.get("snapshot")
                .and_then(|snapshot| snapshot.get("cpu_count"))
                .and_then(Value::as_u64)
                .is_some_and(|count| count > 0),
            "resource snapshot should include cpu_count: {payload}"
        );
        assert!(
            data.get("policy")
                .and_then(|policy| policy.get("hard_load_ratio"))
                .and_then(Value::as_f64)
                .is_some_and(|hard| hard > 0.0),
            "resource policy should include load thresholds: {payload}"
        );
        assert!(
            data.get("admissions")
                .and_then(|admissions| admissions.get("local_model_large"))
                .is_some(),
            "resource admissions should include local_model_large: {payload}"
        );
    }

    #[test]
    fn runtime_snapshot_includes_resource_state() {
        let payload =
            api_payload("/api/v1/runtime/snapshot", "2026-06-02T00:00:00Z").expect("snapshot");
        let data = payload.get("data").expect("data");

        assert!(
            data.get("resource").is_some(),
            "runtime snapshot should include resource state: {payload}"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn parses_macos_memory_pressure_free_percentage_as_available_bytes() {
        let raw = "\
The system has 25769803776 (1572864 pages with a page size of 16384).

System-wide memory free percentage: 53%
";

        let bytes = parse_macos_memory_pressure_available_bytes(raw)
            .expect("parse memory_pressure output");

        assert_eq!(bytes, 13_657_996_001);
    }

    #[test]
    fn capability_catalogs_read_sanitized_local_state() {
        let state = temp_state_dir("capability-catalogs");
        let dir = state.join("capabilities");
        fs::create_dir_all(&dir).expect("create capabilities dir");
        fs::write(
            dir.join("local-capability-inventory-2026-06-03.json"),
            json!({
                "schema_version": "heiwa_local_capability_inventory_v1",
                "providers": [
                    {"provider": "gemini", "version": "0.38.2"}
                ],
                "codex_plugins_observed": ["Browser", "Chrome"],
                "codex_mcp_servers": ["figma", "notion", "node_repl"],
                "installed_apps_observed": ["Codex.app", "Claude.app", "Gemini.app"],
                "reference_sources": ["official.openai.agents-sdk", "official.ollama.api"],
                "integration_families": ["provider_apps", "mcp_servers", "local_models"],
                "runtime_targets": ["rust", "typescript", "wasm"],
                "performance_targets": ["microsecond_readmodel", "bounded_local_worker"],
                "next_runtime_targets": ["api_v1_capabilities_read_model"]
            })
            .to_string(),
        )
        .expect("write capability catalog");

        let payload = capabilities_payload_for_state_dir(&state);

        let catalogs = payload
            .get("catalogs")
            .and_then(Value::as_array)
            .expect("catalogs array");
        assert_eq!(catalogs.len(), 1);
        assert_eq!(
            catalogs[0].get("catalog_id").and_then(Value::as_str),
            Some("local-capability-inventory-2026-06-03")
        );
        assert_eq!(
            catalogs[0]
                .get("schema_version")
                .and_then(Value::as_str),
            Some("heiwa_local_capability_inventory_v1")
        );
        assert_eq!(
            catalogs[0]
                .get("codex_plugins_observed")
                .and_then(Value::as_u64),
            Some(2)
        );
        assert_eq!(
            catalogs[0]
                .get("codex_mcp_servers")
                .and_then(Value::as_u64),
            Some(3)
        );
        assert_eq!(
            catalogs[0]
                .get("installed_apps_observed")
                .and_then(Value::as_u64),
            Some(3)
        );
        assert_eq!(
            catalogs[0]
                .get("reference_sources")
                .and_then(Value::as_u64),
            Some(2)
        );
        assert_eq!(
            catalogs[0]
                .get("integration_families")
                .and_then(Value::as_u64),
            Some(3)
        );
        assert_eq!(
            catalogs[0]
                .get("runtime_targets")
                .and_then(Value::as_u64),
            Some(3)
        );
        assert_eq!(
            catalogs[0]
                .get("performance_targets")
                .and_then(Value::as_u64),
            Some(2)
        );

        let _ = fs::remove_dir_all(&state);
    }
}
