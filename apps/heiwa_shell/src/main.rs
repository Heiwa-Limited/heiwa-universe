use anyhow::{anyhow, Result};
use std::env;
use std::sync::Arc;
use chrono::Utc;
use heiwa_protocol::{SessionState, RoutingState, TranscriptBlock, TurnRequest, parse_turn_intent};
use heiwa_tui::render_cockpit;
use heiwa_core::drex::{default_policy, plan_route, preflight_execution, DrexIngress, ExecutionMode};
use heiwa_provider::adapter::{Message, ProviderAdapter, Role, StreamEvent};
use heiwa_provider::providers::ollama::OllamaCliAdapter;
use heiwa_provider::providers::claude_code::ClaudeCodeCliAdapter;
use heiwa_repl::{parse_input, render_footer, ReplCommand, TelemetryState};
use std::io::{self, Write};

fn provider_supports_loop_adapter(provider: &str) -> bool {
    matches!(provider, "claude" | "ollama")
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RoutePreference {
    Auto,
    LocalOnly,
    RemoteOnly,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        run_repl().await?;
        return Ok(());
    }

    match args[1].as_str() {
        "install" => {
            heiwa_install::run_install()?;
            println!("Registering device...");
            let stdb_client = attempt_stdb_connection().await;
            register_current_device(&stdb_client).await?;
        }
        "login" => {
            if args.len() < 3 {
                println!("Usage: heiwa login [token]");
            } else {
                let identity = heiwa_provider::login_heiwa(&args[2])?;
                println!("Successfully logged in as {} ({})", identity.display_name.as_deref().unwrap_or_default(), identity.user_id);
                
                // Write ~/.heiwa/connection.json with default STDB endpoint
                let heiwa_dir = dirs::home_dir().map(|h| h.join(".heiwa")).expect("HOME must be set");
                let conn_path = heiwa_dir.join("connection.json");
                let conn_json = serde_json::json!({
                    "url": "https://maincloud.spacetimedb.com",
                    "database": "heiwaproductiondb",
                    "token": ""
                });
                std::fs::write(conn_path, serde_json::to_string_pretty(&conn_json)?)?;
                println!("Sync connection initialized. Run 'heiwa register' to sync this device.");
            }
        }
        "logout" => {
            heiwa_provider::clear_identity()?;
            println!("Successfully logged out from Heiwa.");
        }
        "register" => {
            let stdb_client = attempt_stdb_connection().await;
            register_current_device(&stdb_client).await?;
        }
        "receipts" => {
            let stdb_client = attempt_stdb_connection().await;
            if !stdb_client.is_connected() {
                println!("Not connected to SpacetimeDB. Receipts require a live connection.");
                println!("Set STDB_URL and STDB_TOKEN environment variables to enable.");
            } else {
                println!("Connected to SpacetimeDB — run receipts are being recorded.");
                println!("Query receipts via: spacetime sql heiwaproductiondb \"SELECT * FROM runs ORDER BY ended_at DESC LIMIT 10\"");
            }
        }
        "devices" => {
            let manifest_path = heiwa_install::get_heiwa_dir().join("machine.json");
            if manifest_path.exists() {
                let content = std::fs::read_to_string(&manifest_path)?;
                let manifest: serde_json::Value = serde_json::from_str(&content)?;
                println!("Devices:");
                println!("  ID:       {}", manifest["device_id"].as_str().unwrap_or("unknown"));
                println!("  Hostname: {}", manifest["hostname"].as_str().unwrap_or("unknown"));
                println!("  OS:       {}", manifest["os"].as_str().unwrap_or("unknown"));
                println!("  Arch:     {}", manifest["arch"].as_str().unwrap_or("unknown"));
                println!("  Installed: {}", manifest["installed_at"].as_str().unwrap_or("unknown"));

                let stdb_client = attempt_stdb_connection().await;
                if stdb_client.is_connected() {
                    println!("  Sync:     Connected to SpacetimeDB");
                } else {
                    println!("  Sync:     Offline (local only)");
                }
            } else {
                println!("No device registered. Run 'heiwa install' first.");
            }
        }
        "doctor" => {
            let report = heiwa_install::check_installation()?;
            println!("Heiwa Doctor Report:");
            println!("  Rust:   {}", report.rust_version.unwrap_or_else(|| "Not found".to_string()));
            println!("  Node:   {}", report.node_version.unwrap_or_else(|| "Not found".to_string()));
            println!("  Python: {}", report.python_version.unwrap_or_else(|| "Not found".to_string()));
            println!();
            if let Some(identity) = heiwa_provider::load_identity() {
                println!("Heiwa Identity:");
                println!("  User ID: {}", identity.user_id);
                println!("  Email:   {}", identity.email.unwrap_or_else(|| "N/A".to_string()));
            } else {
                println!("Heiwa Identity: Not logged in (run 'heiwa login')");
            }
            println!();
            println!("Providers:");
            println!("  Claude: {}", if report.claude_installed { "Installed" } else { "Not found" });
            println!("  Codex:  {}", if report.codex_installed { "Installed" } else { "Not found" });
            println!("  Gemini: {}", if report.gemini_installed { "Installed" } else { "Not found" });
            println!("  Antigravity: {}", if report.antigravity_installed { "Installed" } else { "Not found" });
            println!("  Ollama: {}", if report.ollama_installed { "Installed" } else { "Not found" });
        }
        "auth" => {
            if args.len() < 3 {
                println!("Usage: heiwa auth [status|login|logout|add-key] [provider] [key]");
            } else {
                match args[2].as_str() {
                    "status" => {
                        // Auto-discover + show registry accounts
                        let mut registry = heiwa_provider::AccountRegistry::load();
                        heiwa_provider::detect::auto_discover(&mut registry).await;
                        if !registry.accounts.is_empty() {
                            println!("Registered Accounts:");
                            for a in &registry.accounts {
                                println!(
                                    "  {:<20} {:<12} ({}) [{:?}] — {} models",
                                    a.account_id, a.provider, a.credential.kind_label(),
                                    a.status, a.models.len(),
                                );
                            }
                            println!();
                        }
                        // Then show legacy CLI discovery
                        let providers = vec!["claude", "codex", "gemini", "antigravity", "ollama"];
                        println!("CLI Discovery:");
                        for p in providers {
                            if let Some(status) = heiwa_provider::get_auth_status(p) {
                                let loop_capable = if provider_supports_loop_adapter(p) { " [loop]" } else { "" };
                                println!("  {:<12} {:<20} ({:?}){}", p, status.status, status.auth_kind, loop_capable);
                            }
                        }
                    }
                    "add-key" => {
                        if args.len() < 5 {
                            println!("Usage: heiwa auth add-key <provider> <api-key>");
                            println!();
                            println!("Providers: anthropic, openai, google, openrouter");
                        } else {
                            let provider = &args[3];
                            let api_key = &args[4];
                            let rate_group = match provider.as_str() {
                                "anthropic" => "anthropic_api",
                                "openai" => "openai_api",
                                "google" => "google_api",
                                "openrouter" => "openrouter",
                                _ => provider.as_str(),
                            };

                            let mut registry = heiwa_provider::AccountRegistry::load();
                            match heiwa_provider::registry::add_api_key_account(
                                &mut registry, provider, api_key, rate_group,
                            ) {
                                Ok(account_id) => {
                                    println!("Stored {} API key in Keychain as '{}'", provider, account_id);
                                    // Verify key and detect models
                                    print!("Verifying...");
                                    io::stdout().flush()?;
                                    if let Some(account) = registry.accounts.iter_mut()
                                        .find(|a| a.account_id == account_id)
                                    {
                                        match heiwa_provider::detect::verify_api_key(account).await {
                                            Ok(()) => {
                                                println!(" {} models available", account.models.len());
                                                for m in &account.models {
                                                    println!("  {} (class:{})", m.model_id, m.capability_class);
                                                }
                                                registry.save()?;
                                            }
                                            Err(e) => {
                                                println!(" verification failed: {}", e);
                                                registry.save()?;
                                            }
                                        }
                                    }
                                }
                                Err(e) => eprintln!("Failed to store key: {}", e),
                            }
                        }
                    }
                    "login" => {
                        if args.len() < 4 {
                            println!("Usage: heiwa auth login [provider]");
                        } else {
                            heiwa_provider::login(&args[3])?;
                        }
                    }
                    "logout" => {
                        if args.len() < 4 {
                            println!("Usage: heiwa auth logout [provider]");
                        } else {
                            heiwa_provider::logout(&args[3])?;
                        }
                    }
                    _ => println!("Unknown auth subcommand: {}", args[2]),
                }
            }
        }
        "providers" => {
            let mut registry = heiwa_provider::AccountRegistry::load();
            // Auto-discover local providers (Ollama, etc.)
            let discoveries = heiwa_provider::detect::auto_discover(&mut registry).await;
            for d in &discoveries {
                println!("  [auto] {}", d);
            }

            if !registry.accounts.is_empty() {
                println!("Provider Accounts:");
                for account in &registry.accounts {
                    let model_count = account.models.len();
                    let loop_cap = if provider_supports_loop_adapter(&account.provider) { " [loop]" } else { "" };
                    println!(
                        "  {:<20} {} ({}) [{:?}] — {} model{}{}",
                        account.account_id,
                        account.provider,
                        account.credential.kind_label(),
                        account.status,
                        model_count,
                        if model_count == 1 { "" } else { "s" },
                        loop_cap,
                    );
                }
            }

            // Show discoverable CLIs not yet in the registry
            let cli_providers = vec!["claude", "codex", "gemini", "antigravity"];
            let mut unregistered = Vec::new();
            for p in cli_providers {
                if let Some(status) = heiwa_provider::get_auth_status(p) {
                    let in_registry = registry.accounts.iter().any(|a| a.provider == p);
                    if !in_registry {
                        let loop_cap = if provider_supports_loop_adapter(p) { " [loop]" } else { "" };
                        unregistered.push(format!(
                            "  {:<20} {} ({:?}){}", p, status.status, status.auth_kind, loop_cap,
                        ));
                    }
                }
            }
            if !unregistered.is_empty() {
                println!("CLI Discovery:");
                for line in &unregistered {
                    println!("{}", line);
                }
            }

            if registry.accounts.is_empty() && unregistered.is_empty() {
                println!("No providers connected.");
                println!("  heiwa auth add-key <provider> <key>  — register an API key");
            }
        }
        "models" => {
            let mut registry = heiwa_provider::AccountRegistry::load();
            heiwa_provider::detect::auto_discover(&mut registry).await;
            let models = registry.all_models();
            if models.is_empty() {
                println!("No models detected. Connect a provider first:");
                println!("  heiwa auth add-key anthropic <your-api-key>");
                println!("  heiwa auth add-key openai <your-api-key>");
            } else {
                let mut current_group = String::new();
                for m in &models {
                    if m.rate_group != current_group {
                        current_group = m.rate_group.clone();
                        let account = registry.get(&m.account_id);
                        let kind = account.map(|a| a.credential.kind_label()).unwrap_or("unknown");
                        println!("\n  {} ({}) [rate: {}]", m.provider, kind, m.rate_group);
                    }
                    let truth_marker = match m.inventory_truth {
                        heiwa_provider::InventoryTruth::Verified => "",
                        heiwa_provider::InventoryTruth::Inferred => " ~inferred",
                        heiwa_provider::InventoryTruth::UserConfigured => " *user",
                    };
                    println!(
                        "    {:<24} class:{}  {:>6} ctx  ${:.4}/1k in{}",
                        m.model_id,
                        m.capability_class,
                        format_context(m.context_window),
                        m.cost_per_1k_input,
                        truth_marker,
                    );
                }
                println!();
            }
        }
        "session" => {
            if args.len() >= 3 && args[2] == "attach" {
                println!("Running session attach...");
            } else {
                println!("Usage: heiwa session attach");
            }
        }
        "loop" => {
            if args.len() < 3 {
                println!("Usage: heiwa loop [max_turns] \"objective\" [--intent code] [--risk low] [--privacy standard]");
            } else {
                let max_turns = args[2].parse::<u32>().unwrap_or(10);
                let objective = if args.len() >= 4 { args[3..].join(" ") } else { "no objective provided".to_string() };
                
                let identity = heiwa_provider::load_identity().ok_or_else(|| anyhow!("Not logged in. Please run 'heiwa login' first."))?;
                
                let intent = if let Some(i) = args.iter().position(|a| a == "--intent") { args[i+1].clone() } else { "code".to_string() };
                let risk = if let Some(i) = args.iter().position(|a| a == "--risk") { args[i+1].clone() } else { "low".to_string() };
                let privacy = if let Some(i) = args.iter().position(|a| a == "--privacy") { args[i+1].clone() } else { "standard".to_string() };

                let config = heiwa_loop::LoopConfig {
                    user_id: identity.user_id,
                    objective,
                    max_turns,
                    max_cost_usd: 1.0,
                    intent,
                    risk,
                    privacy,
                    runtime: "any".to_string(),
                };

                let mut registry = heiwa_provider::AccountRegistry::load();
                heiwa_provider::detect::auto_discover(&mut registry).await;
                let model_tiers = get_live_model_tiers(&registry);
                if model_tiers.is_empty() {
                    return Err(anyhow!("No loop-capable models found. Run 'heiwa providers' to check."));
                }

                // Try to connect to STDB if environment allows
                let stdb_client = attempt_stdb_connection().await;

                let controller = heiwa_loop::LoopController::new(config, stdb_client, model_tiers);
                let (tx, mut rx) = tokio::sync::mpsc::channel(10);
                
                println!("Loop initiated: {}", controller.get_id());
                
                let adapters: Arc<dyn Fn(&str) -> Option<Arc<dyn ProviderAdapter>> + Send + Sync> = Arc::new(|provider: &str| {
                    match provider {
                        "ollama" => Some(Arc::new(OllamaCliAdapter::new()) as Arc<dyn ProviderAdapter>),
                        "claude" => Some(Arc::new(ClaudeCodeCliAdapter::new()) as Arc<dyn ProviderAdapter>),
                        _ => None,
                    }
                });

                let c = controller;
                tokio::spawn(async move {
                    if let Err(e) = c.run(tx, adapters).await {
                        eprintln!("Loop error: {}", e);
                    }
                });
                
                while let Some(status) = rx.recv().await {
                    println!("[{}] Turn: {} | Cost: ${:.4}", status.status, status.current_turn, status.total_cost_usd);
                    if status.status == "COMPLETED" || status.status == "CANCELLED" || status.status == "FAILED" {
                        break;
                    }
                }
            }
        }
        "shell" => {
            run_repl().await?;
        }
        "--help" | "-h" | "help" => {
            print_help();
        }
        _ => {
            println!("Heiwa AI runtime and shell");
            println!("Unknown command: {}", args[1]);
            print_help();
        }
    }

    Ok(())
}

fn print_help() {
    println!("Heiwa — BYOK terminal agent");
    println!();
    println!("Usage: heiwa [COMMAND]");
    println!();
    println!("Commands:");
    println!("  install                       Install Heiwa and its dependencies");
    println!("  login [token]                 Sign in to Heiwa");
    println!("  logout                        Sign out from Heiwa");
    println!("  doctor                        Check the status of the Heiwa installation");
    println!("  register                      Register the current device");
    println!("  receipts                      Show run receipt status");
    println!("  devices                       Show registered devices");
    println!("  auth status                   Show all connected accounts and CLI discovery");
    println!("  auth add-key <provider> <key> Register an API key for a provider");
    println!("  auth login <provider>         Login to a provider CLI");
    println!("  auth logout <provider>        Logout from a provider CLI");
    println!("  providers                     List connected accounts and models");
    println!("  models                        List all detected models by rate group");
    println!("  session attach                Attach to a Heiwa session");
    println!("  loop [turns] <objective>      Run a bounded execution loop");
    println!("  shell                         Enter interactive mode");
    println!("  help                          Print this message");
}

fn format_context(tokens: u32) -> String {
    if tokens >= 1_000_000 {
        format!("{}M", tokens / 1_000_000)
    } else if tokens >= 1_000 {
        format!("{}k", tokens / 1_000)
    } else {
        format!("{}", tokens)
    }
}

async fn register_current_device(stdb_client: &heiwa_stdb::StdbClient) -> Result<()> {
    let identity = match heiwa_provider::load_identity() {
        Some(id) => id,
        None => {
            println!("Not logged in. Please run 'heiwa login' first.");
            return Ok(());
        }
    };

    let _report = heiwa_install::check_installation()?;
    let manifest_path = heiwa_install::get_heiwa_dir().join("machine.json");
    
    let device_id = if manifest_path.exists() {
        let content = std::fs::read_to_string(&manifest_path)?;
        let manifest: serde_json::Value = serde_json::from_str(&content)?;
        manifest["device_id"].as_str().unwrap_or("unknown").to_string()
    } else {
        "unknown".to_string()
    };

    let hostname = hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    println!("Registering device {} for user {}...", device_id, identity.user_id);

    stdb_client.register_device(
        &device_id,
        &identity.user_id,
        &hostname,
        std::env::consts::OS,
        std::env::consts::ARCH,
    )?;

    // Sync provider statuses
    let mut registry = heiwa_provider::AccountRegistry::load();
    heiwa_provider::detect::auto_discover(&mut registry).await;
    for account in &registry.accounts {
        let models_json = serde_json::to_string(&account.models).unwrap_or_else(|_| "[]".to_string());
        stdb_client.sync_provider_status(
            &account.account_id,
            &account.provider,
            &device_id,
            account.credential.kind_label(),
            &account.account_id, // local_handle_ref
            &format!("{:?}", account.status),
            None,
            None,
            &models_json,
        )?;
        println!("  Synced provider {} status: {:?}", account.provider, account.status);
    }

    if stdb_client.is_connected() {
        println!("Device and capabilities synced to SpacetimeDB.");
    } else {
        println!("Device registered locally (STDB offline — will sync when connected).");
    }
    Ok(())
}

fn get_live_model_tiers(registry: &heiwa_provider::AccountRegistry) -> Vec<heiwa_bindings::ModelTier> {
    registry
        .all_models()
        .into_iter()
        .filter(|m| provider_supports_loop_adapter(&m.provider))
        .map(|m| {
            let mut strengths = vec!["chat"];
            if m.supports_tools { strengths.push("tool_use"); }
            if m.supports_vision { strengths.push("vision"); }
            if m.capability_class >= 4 { strengths.push("advanced_coding"); }

            heiwa_bindings::ModelTier {
                id: 0,
                model_id: m.model_id.clone(),
                provider_model_id: m.provider_model_id.clone(),
                provider: m.provider.clone(),
                rate_group: m.rate_group.clone(),
                capability_class: m.capability_class,
                effort_knob: "default".to_string(),
                effort_level: 1,
                cost_per_turn: m.cost_per_1k_input * 4.0, // ~4k tokens/turn estimate
                max_context_tokens: m.context_window,
                strengths_json: serde_json::to_string(&strengths).unwrap_or_default(),
                vram_requirement_mb: 0,
                quantization_type: "none".to_string(),
                kv_cache_strategy: "standard".to_string(),
                enabled: true,
                last_success_rate: 1.0,
                avg_latency_ms: if m.rate_group == "local" { 50 } else { 200 },
                latency_p_95_ms: if m.rate_group == "local" { 100 } else { 500 },
                updated_at: Utc::now().to_rfc3339(),
            }
        })
        .collect()
}

async fn attempt_stdb_connection() -> heiwa_stdb::StdbClient {
    match heiwa_stdb::StdbConfig::resolve() {
        Some(config) => {
            let client = heiwa_stdb::StdbClient::connect(&config);
            client.spawn_advance_loop();
            client
        }
        None => heiwa_stdb::StdbClient::offline(),
    }
}

async fn run_repl() -> Result<()> {
    println!("Heiwa Interactive Shell");
    println!("Type /help for commands, !command for shell escape, or enter a task.");
    println!();

    let stdb_client = attempt_stdb_connection().await;
    if stdb_client.is_connected() {
        println!("  Connected to SpacetimeDB");
    } else if heiwa_provider::load_identity().is_some() {
        println!("  SpacetimeDB unreachable — running offline");
    } else {
        println!("  Not logged in — run 'heiwa login' to enable sync");
    }

    // Start device heartbeat if connected
    let heartbeat_device_id = {
        let manifest_path = heiwa_install::get_heiwa_dir().join("machine.json");
        if manifest_path.exists() {
            std::fs::read_to_string(&manifest_path)
                .ok()
                .and_then(|c| serde_json::from_str::<serde_json::Value>(&c).ok())
                .and_then(|m| m["device_id"].as_str().map(|s| s.to_string()))
        } else {
            None
        }
    };

    if let Some(ref dev_id) = heartbeat_device_id {
        let stdb_hb = stdb_client.clone();
        let dev_id_clone = dev_id.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
            loop {
                interval.tick().await;
                let _ = stdb_hb.heartbeat_device(&dev_id_clone);
            }
        });
    }

    // Load registry once at REPL start
    let mut registry = heiwa_provider::AccountRegistry::load();
    heiwa_provider::detect::auto_discover(&mut registry).await;
    let model_tiers = get_live_model_tiers(&registry);

    if model_tiers.is_empty() {
        println!("No loop-capable models available. Run 'heiwa providers' or 'heiwa auth add-key' to connect.");
    }

    let mut state = SessionState {
        session_id: "default".to_string(),
        transcript: vec![],
        routing: RoutingState {
            current_provider: "none".to_string(),
            current_model: "none".to_string(),
            mode: "Auto".to_string(),
            explanation: None,
        },
        devices: vec![],
        receipts: vec![],
    };

    let mut turn_count = 0;
    let mut current_provider = String::new();
    let mut current_model = String::new();
    let mut pinned_provider: Option<String> = None;
    let mut pinned_model: Option<String> = None;
    let mut route_preference = RoutePreference::Auto;

    // Set initial telemetry from first available model tier
    if let Some(first) = model_tiers.first() {
        current_provider = first.provider.clone();
        current_model = first.model_id.clone();
        state.routing.current_provider = current_provider.clone();
        state.routing.current_model = current_model.clone();
    }

    loop {
        let footer_state = TelemetryState {
            provider: if current_provider.is_empty() { "none".to_string() } else { current_provider.clone() },
            model: if current_model.is_empty() { "none".to_string() } else { current_model.clone() },
            route: current_route_label(route_preference, pinned_provider.as_deref(), pinned_model.as_deref()),
            status: "ready".to_string(),
            turn_count,
            loop_info: None,
        };

        print!("\r{}", render_footer(&footer_state));
        print!("\n> ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let input = input.trim();

        if input == "exit" || input == "quit" {
            break;
        }

        let cmd = parse_input(input);
        match cmd {
            ReplCommand::Task(t) => {
                if t.is_empty() { continue; }
                let request_id = uuid::Uuid::new_v4().to_string();
                let turn_started_at = chrono::Utc::now().to_rfc3339();

                // 1. Direct Turn Instruction
                let turn_request = parse_turn_intent(&t);
                let (provider_pin, model_pin) = match (turn_request.provider_pin, turn_request.model_pin) {
                    (Some(p), Some(m)) => (Some(p), Some(m)),
                    (Some(p), None) => (Some(p), None),
                    _ => (None, None),
                };

                // 2. Session Override
                let final_provider_pin = provider_pin.as_deref().or(pinned_provider.as_deref());
                let final_model_pin = model_pin.as_deref().or(pinned_model.as_deref());

                // 3. Filter model tiers by precedence
                let routed_tiers = filtered_model_tiers(
                    &model_tiers,
                    route_preference,
                    final_provider_pin,
                    final_model_pin,
                );

                if routed_tiers.is_empty() {
                    let reason = if final_model_pin.is_some() {
                        format!("Model '{}' not available.", final_model_pin.unwrap())
                    } else if final_provider_pin.is_some() {
                        format!("Provider '{}' not available.", final_provider_pin.unwrap())
                    } else {
                        "No models available.".to_string()
                    };
                    println!("Routing failed: {}", reason);
                    continue;
                }

                // DREX route the task
                let ingress = DrexIngress {
                    intent: infer_task_intent(&t),
                    risk: "low".to_string(),
                    raw_text: t.clone(),
                    privacy: "standard".to_string(),
                    runtime: runtime_for_route_preference(route_preference).to_string(),
                    available_vram_mb: 8192,
                    required_context_tokens: 1024,
                };

                let policy = default_policy();
                let preflight = preflight_execution(&ingress, &routed_tiers, &policy);
                
                // 4. Ban silent fallback: If DREX preflight says a mode that's not possible with our pins, fail.
                match preflight.execution_mode {
                    ExecutionMode::Deterministic | ExecutionMode::Clarify => {
                        if let Some(response) = preflight.response_text {
                            println!("{}", response);
                        }
                        turn_count += 1;
                        continue;
                    }
                    _ => {}
                }

                let effective_route_preference = if route_preference == RoutePreference::Auto {
                    match preflight.execution_mode {
                        ExecutionMode::LocalModel => RoutePreference::LocalOnly,
                        ExecutionMode::RemoteModel => RoutePreference::RemoteOnly,
                        _ => RoutePreference::Auto,
                    }
                } else {
                    route_preference
                };

                let effective_tiers = filtered_model_tiers(
                    &routed_tiers,
                    effective_route_preference,
                    final_provider_pin,
                    final_model_pin,
                );

                if effective_tiers.is_empty() {
                    println!("Routing failed: Chosen model stack unavailable for this execution mode.");
                    continue;
                }

                let route = match plan_route(&ingress, &effective_tiers, &policy) {
                    Ok(r) => r,
                    Err(e) => {
                        eprintln!("Routing failed: {}", e);
                        continue;
                    }
                };

                let selected = match route.selected_model {
                    Some(ref m) => m,
                    None => {
                        eprintln!("No model matched for this task.");
                        continue;
                    }
                };

                let _ = stdb_client.record_route_decision(
                    &request_id,
                    &request_id, // task_id = request_id for REPL tasks
                    &t,
                    &infer_task_intent(&t),
                    "low",
                    "standard",
                    &selected.provider,
                    &selected.provider,
                    &selected.model_id,
                    if is_local_provider_check(&selected.provider) { "local" } else { "remote" },
                    &route.routing_metadata,
                    0.9,
                );

                // Resolve adapter for provider
                let adapter: Arc<dyn ProviderAdapter> = match selected.provider.as_str() {
                    "ollama" => Arc::new(OllamaCliAdapter::with_model(&selected.model_id)),
                    "claude" => Arc::new(ClaudeCodeCliAdapter::new()),
                    _ => {
                        eprintln!("No adapter for provider '{}' yet.", selected.provider);
                        continue;
                    }
                };

                current_provider = selected.provider.clone();
                current_model = selected.model_id.clone();
                let selected_model_id = selected.model_id.clone();

                state.transcript.push(TranscriptBlock::User(t.clone()));

                // Stream the response
                let messages = vec![Message { role: Role::User, content: t }];
                let (stream_tx, mut stream_rx) = tokio::sync::mpsc::channel(32);
                let model_id = selected.provider_model_id.clone();

                tokio::spawn({
                    let adapter = adapter.clone();
                    async move {
                        if let Err(e) = adapter.send(&model_id, &messages, stream_tx).await {
                            eprintln!("Adapter error: {}", e);
                        }
                    }
                });

                // Print tokens as they arrive
                let mut usage = None;
                let mut full_response = String::new();
                while let Some(event) = stream_rx.recv().await {
                    match event {
                        StreamEvent::Token(text) => {
                            print!("{}", text);
                            io::stdout().flush()?;
                            full_response.push_str(&text);
                        }
                        StreamEvent::Done(u) => {
                            usage = Some(u);
                            break;
                        }
                        StreamEvent::Error(e) => {
                            eprintln!("\nStream error: {}", e);
                            break;
                        }
                        StreamEvent::ToolUse { name, .. } => {
                            println!("\n[tool: {}]", name);
                            state.transcript.push(TranscriptBlock::Tool(name, "executed".to_string()));
                        }
                    }
                }
                println!(); // newline after streamed output
                state.transcript.push(TranscriptBlock::Assistant(full_response));

                let turn_ended_at = chrono::Utc::now().to_rfc3339();
                let user_id = heiwa_provider::load_identity()
                    .map(|id| id.user_id)
                    .unwrap_or_else(|| "anonymous".to_string());

                if let Some(u) = usage {
                    let _ = stdb_client.record_run(
                        &format!("run-{}", uuid::Uuid::new_v4()),
                        &user_id,
                        &request_id,
                        &turn_started_at,
                        &turn_ended_at,
                        "SUCCESS",
                        &selected_model_id,
                        u.input_tokens as i64,
                        u.output_tokens as i64,
                        u.cost_usd,
                        None,
                        None,
                        None,
                    );
                    if u.input_tokens > 0 || u.cost_usd > 0.0 {
                        println!(
                            "  [{} in / {} out | ${:.4}]",
                            u.input_tokens, u.output_tokens, u.cost_usd
                        );
                    }
                } else {
                    let _ = stdb_client.record_run(
                        &format!("run-{}", uuid::Uuid::new_v4()),
                        &user_id,
                        &request_id,
                        &turn_started_at,
                        &turn_ended_at,
                        "COMPLETED_NO_USAGE",
                        &selected_model_id,
                        0,
                        0,
                        0.0,
                        None,
                        None,
                        None,
                    );
                }
                turn_count += 1;
            }
            ReplCommand::Shell(s) => {
                println!("Escaping to shell: {}", s);
                let output = std::process::Command::new("sh")
                    .arg("-c")
                    .arg(s)
                    .output();
                match output {
                    Ok(o) => {
                        io::stdout().write_all(&o.stdout)?;
                        io::stderr().write_all(&o.stderr)?;
                    }
                    Err(e) => eprintln!("Shell error: {}", e),
                }
            }
            ReplCommand::Slash(c, args) => {
                match c.as_str() {
                    "help" => println!("Available slash commands: /auth, /providers, /provider, /models, /model, /route, /status, /clear, /loop, /exit"),
                    "auth" => println!("Manage auth via 'heiwa auth'"),
                    "providers" => {
                        let mut reg = heiwa_provider::AccountRegistry::load();
                        heiwa_provider::detect::auto_discover(&mut reg).await;
                        let tiers = get_live_model_tiers(&reg);
                        for t in tiers {
                            println!("  {} ({}) class:{}", t.model_id, t.provider, t.capability_class);
                        }
                    }
                    "provider" => {
                        let available_providers = available_providers(&model_tiers);
                        match args.first().map(|s| s.as_str()) {
                            None => {
                                let active = pinned_provider.as_deref().unwrap_or("auto");
                                println!("Current provider routing: {}", active);
                                if available_providers.is_empty() {
                                    println!("No loop-capable providers available.");
                                } else {
                                    println!("Available providers:");
                                    for provider in available_providers {
                                        println!("  {}", provider);
                                    }
                                }
                            }
                            Some("auto") | Some("clear") => {
                                pinned_provider = None;
                                pinned_model = None;
                                println!("Provider routing reset to automatic.");
                            }
                            Some(provider) => {
                                if available_providers.iter().any(|p| p == provider) {
                                    pinned_provider = Some(provider.to_string());
                                    if let Some(model) = pinned_model.as_ref() {
                                        let matches_provider = model_tiers.iter().any(|tier| {
                                            tier.model_id == *model && tier.provider == provider
                                        });
                                        if !matches_provider {
                                            pinned_model = None;
                                        }
                                    }
                                    println!("Pinned provider to {}.", provider);
                                } else {
                                    println!("Unknown provider '{}'.", provider);
                                }
                            }
                        }
                    }
                    "models" => {
                        if model_tiers.is_empty() {
                            println!("No loop-capable models available.");
                        } else {
                            for tier in &model_tiers {
                                println!("  {} ({}) class:{}", tier.model_id, tier.provider, tier.capability_class);
                            }
                        }
                    }
                    "model" => {
                        match args.first().map(|s| s.as_str()) {
                            None => {
                                let active = pinned_model.as_deref().unwrap_or("auto");
                                println!("Current model routing: {}", active);
                                if model_tiers.is_empty() {
                                    println!("No loop-capable models available.");
                                } else {
                                    println!("Available models:");
                                    for tier in &model_tiers {
                                        println!("  {} ({})", tier.model_id, tier.provider);
                                    }
                                }
                            }
                            Some("auto") | Some("clear") => {
                                pinned_model = None;
                                println!("Model routing reset to automatic.");
                            }
                            Some(model_id) => {
                                if let Some(tier) = model_tiers.iter().find(|tier| {
                                    tier.model_id == model_id || tier.provider_model_id == model_id
                                }) {
                                    pinned_model = Some(tier.model_id.clone());
                                    pinned_provider = Some(tier.provider.clone());
                                    println!("Pinned model to {} ({}).", tier.model_id, tier.provider);
                                } else {
                                    println!("Unknown model '{}'.", model_id);
                                }
                            }
                        }
                    }
                    "route" => {
                        match args.first().map(|s| s.as_str()) {
                            None => {
                                println!("Current route preference: {}", route_preference_label(route_preference));
                                println!("Options: /route auto | /route local | /route remote");
                            }
                            Some("auto") => {
                                route_preference = RoutePreference::Auto;
                                println!("Route preference reset to automatic.");
                            }
                            Some("local") => {
                                route_preference = RoutePreference::LocalOnly;
                                println!("Route preference set to local-only.");
                            }
                            Some("remote") => {
                                route_preference = RoutePreference::RemoteOnly;
                                println!("Route preference set to remote-only.");
                            }
                            Some(other) => {
                                println!("Unknown route preference '{}'.", other);
                            }
                        }
                    }
                    "status" => {
                        println!("Session status:");
                        println!(
                            "  provider: {}",
                            if current_provider.is_empty() { "none" } else { &current_provider }
                        );
                        println!(
                            "  model: {}",
                            if current_model.is_empty() { "none" } else { &current_model }
                        );
                        println!("  route: {}", route_preference_label(route_preference));
                        println!(
                            "  pinned provider: {}",
                            pinned_provider.as_deref().unwrap_or("auto")
                        );
                        println!("  pinned model: {}", pinned_model.as_deref().unwrap_or("auto"));
                        println!("  turn count: {}", turn_count);
                        println!("  available loop models: {}", model_tiers.len());
                    }
                    "clear" => {
                        pinned_provider = None;
                        pinned_model = None;
                        route_preference = RoutePreference::Auto;
                        println!("Cleared route, provider, and model session pins.");
                    }
                    "loop" => {
                        let max_turns = args.get(0).and_then(|s| s.parse::<u32>().ok()).unwrap_or(5);
                        let objective = if args.len() > 1 { args[1..].join(" ") } else { "explore context".to_string() };
                        
                        println!("Starting loop: '{}' ({} turns)", objective, max_turns);
                        
                        let identity = heiwa_provider::load_identity().unwrap_or(heiwa_provider::HeiwaIdentity {
                            user_id: "anonymous".to_string(),
                            auth_token: "".to_string(),
                            email: None,
                            display_name: None,
                        });

                        let config = heiwa_loop::LoopConfig {
                            user_id: identity.user_id,
                            objective,
                            max_turns,
                            max_cost_usd: 1.0,
                            intent: "research".to_string(),
                            risk: "low".to_string(),
                            privacy: "standard".to_string(),
                            runtime: "any".to_string(),
                        };

                        let mut reg = heiwa_provider::AccountRegistry::load();
                        heiwa_provider::detect::auto_discover(&mut reg).await;
                        let model_tiers = get_live_model_tiers(&reg);
                        
                        let controller = heiwa_loop::LoopController::new(config, stdb_client.clone(), model_tiers);
                        let (tx, mut rx) = tokio::sync::mpsc::channel(10);

                        let adapters: Arc<dyn Fn(&str) -> Option<Arc<dyn ProviderAdapter>> + Send + Sync> = Arc::new(|provider: &str| {
                            match provider {
                                "ollama" => Some(Arc::new(OllamaCliAdapter::new()) as Arc<dyn ProviderAdapter>),
                                "claude" => Some(Arc::new(ClaudeCodeCliAdapter::new()) as Arc<dyn ProviderAdapter>),
                                _ => None,
                            }
                        });

                        tokio::spawn(async move {
                            let _ = controller.run(tx, adapters).await;
                        });

                        while let Some(status) = rx.recv().await {
                            let telemetry = TelemetryState {
                                provider: current_provider.clone(),
                                model: current_model.clone(),
                                route: current_route_label(route_preference, pinned_provider.as_deref(), pinned_model.as_deref()),
                                status: status.status.clone(),
                                turn_count,
                                loop_info: Some((status.current_turn, max_turns)),
                            };
                            print!("\r{}\r", render_footer(&telemetry));
                            io::stdout().flush()?;
                            
                            if status.status == "COMPLETED" || status.status == "CANCELLED" || status.status == "FAILED" {
                                println!("\nLoop finished: {}", status.status);
                                break;
                            }
                        }
                    }
                    _ => println!("Unknown slash command: /{}", c),
                }
            }
        }
    }

    Ok(())
}

fn infer_task_intent(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return "chat".to_string();
    }

    let lowercase = trimmed.to_ascii_lowercase();
    let code_keywords = [
        "refactor", "function", "crate", "cargo", "rust", "python", "typescript", "javascript",
        "code", "bug", "test", "file", "repo", "implement", "fix", "patch", "compile", "build",
        "shell", "command", "cli", "adapter", "stream", "binary", "terminal",
    ];
    if code_keywords.iter().any(|needle| lowercase.contains(needle))
        || trimmed.contains("::")
        || trimmed.contains('/')
        || trimmed.contains('`')
    {
        return "code".to_string();
    }

    "chat".to_string()
}

fn filtered_model_tiers(
    model_tiers: &[heiwa_bindings::ModelTier],
    route_preference: RoutePreference,
    pinned_provider: Option<&str>,
    pinned_model: Option<&str>,
) -> Vec<heiwa_bindings::ModelTier> {
    model_tiers
        .iter()
        .filter(|tier| match route_preference {
            RoutePreference::Auto => true,
            RoutePreference::LocalOnly => is_local_provider(&tier.provider),
            RoutePreference::RemoteOnly => !is_local_provider(&tier.provider),
        })
        .filter(|tier| pinned_provider.map(|provider| tier.provider == provider).unwrap_or(true))
        .filter(|tier| pinned_model.map(|model| tier.model_id == model).unwrap_or(true))
        .cloned()
        .collect()
}

fn available_providers(model_tiers: &[heiwa_bindings::ModelTier]) -> Vec<String> {
    let mut providers = Vec::new();
    for tier in model_tiers {
        if !providers.contains(&tier.provider) {
            providers.push(tier.provider.clone());
        }
    }
    providers
}

fn current_route_label(
    route_preference: RoutePreference,
    pinned_provider: Option<&str>,
    pinned_model: Option<&str>,
) -> String {
    if let Some(model) = pinned_model {
        format!("model:{}", model)
    } else if let Some(provider) = pinned_provider {
        format!("provider:{}", provider)
    } else if route_preference != RoutePreference::Auto {
        format!("route:{}", route_preference_label(route_preference))
    } else {
        "direct".to_string()
    }
}

fn route_preference_label(route_preference: RoutePreference) -> &'static str {
    match route_preference {
        RoutePreference::Auto => "auto",
        RoutePreference::LocalOnly => "local",
        RoutePreference::RemoteOnly => "remote",
    }
}

fn runtime_for_route_preference(route_preference: RoutePreference) -> &'static str {
    match route_preference {
        RoutePreference::LocalOnly => "local",
        RoutePreference::RemoteOnly | RoutePreference::Auto => "any",
    }
}

fn is_local_provider(provider: &str) -> bool {
    matches!(provider, "ollama" | "local" | "vllm" | "litellm")
}

fn is_local_provider_check(provider: &str) -> bool {
    is_local_provider(provider)
}

#[cfg(test)]
mod tests {
    use super::infer_task_intent;

    #[test]
    fn greeting_input_defaults_to_chat_intent() {
        assert_eq!(infer_task_intent("hi"), "chat");
        assert_eq!(infer_task_intent("hello there"), "chat");
    }

    #[test]
    fn coding_input_uses_code_intent() {
        assert_eq!(infer_task_intent("refactor this Rust function"), "code");
        assert_eq!(infer_task_intent("fix the failing cargo test"), "code");
    }
}
