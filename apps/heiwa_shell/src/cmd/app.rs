use anyhow::{anyhow, Result};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use serde_json::{json, Value};
use sha1::{Digest, Sha1};
use std::env;
use std::fs;
use std::net::SocketAddr;
use std::path::{Component, Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;
use tokio::time::{self, Duration};

const DEFAULT_PORT: u16 = 7474;
const HEARTBEAT_TTL_SECS: i64 = 120;
const WS_GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

pub async fn run(args: &[String]) -> Result<()> {
    match args.first().map(String::as_str) {
        Some("start") => start(&args[1..]).await,
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
                "workers": status.workers_summary,
                "approvals": status.approvals_summary,
                "mail": status.mail_summary,
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
    workers_summary: Value,
    approvals_summary: Value,
    mail_summary: Value,
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
            workers_summary: workers_summary(&state_dir),
            approvals_summary: approvals_summary(&state_dir),
            mail_summary: mail_summary(),
        }
    }
}

async fn handle_connection(mut stream: TcpStream, started_at: Arc<String>) -> Result<()> {
    let request = read_http_request(&mut stream).await?;
    if request.is_empty() {
        return Ok(());
    }

    if is_websocket_request(&request) {
        return handle_websocket(stream, &request, started_at).await;
    }

    let method = request_method(&request).unwrap_or("GET");
    let path = request_path(&request).unwrap_or("/");
    if method == "OPTIONS" {
        return write_response(&mut stream, 204, "text/plain", Vec::new(), false).await;
    }
    let head_only = method == "HEAD";
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

async fn read_http_request(stream: &mut TcpStream) -> Result<String> {
    let mut data = Vec::new();
    let mut buf = [0u8; 1024];
    loop {
        let n = stream.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        data.extend_from_slice(&buf[..n]);
        if data.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
        if data.len() > 64 * 1024 {
            return Err(anyhow!("request headers too large"));
        }
    }
    Ok(String::from_utf8_lossy(&data).to_string())
}

async fn handle_websocket(
    mut stream: TcpStream,
    request: &str,
    started_at: Arc<String>,
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
         Access-Control-Allow-Methods: GET, OPTIONS\r\n\
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
        "/api/v1/missions" => json!({ "missions": [], "cursor": null }),
        "/api/v1/approvals" => json!({ "approvals": approval_rows() }),
        "/api/v1/rate-groups" => json!({ "rate_groups": rate_group_rows() }),
        "/api/v1/history" => json!({
            "sessions": [],
            "recent_runs": [],
            "artifacts": [],
            "cursor": null,
        }),
        "/api/v1/traces" => json!({ "traces": [] }),
        "/api/v1/memory" => json!({ "entries": [] }),
        "/api/v1/agents" => json!({ "agents": worker_agent_rows() }),
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
        "providers": provider_rows(),
    })
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
    println!("  heiwa app runtime status [--json]");
    println!("  heiwa app status [--json]");
    println!("  heiwa app [--json]");
    println!();
    println!("Starts or probes the local Heiwa.app cockpit runtime.");
}

fn print_start_help() {
    println!("heiwa app start");
    println!();
    println!("Usage:");
    println!("  heiwa app start [--port N] [--no-open]");
    println!();
    println!("Binds 127.0.0.1, serves the cockpit, opens the browser by default,");
    println!("starts caffeinate while running, and writes a worker heartbeat.");
}
