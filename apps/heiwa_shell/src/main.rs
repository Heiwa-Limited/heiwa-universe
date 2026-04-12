use anyhow::{anyhow, Result};
use std::env;
use std::sync::Arc;
use chrono::Utc;
use heiwa_paths::RuntimePaths;
use heiwa_protocol::{
    CockpitCommand, CockpitEvent, SessionState, RoutingState, TranscriptBlock, parse_turn_intent,
};
use heiwa_core::drex::{default_policy, plan_route, preflight_execution, DrexIngress, ExecutionMode};
use heiwa_provider::adapter::{Message, ProviderAdapter, Role, StreamEvent, TokenUsage};
use heiwa_provider::providers::ollama::OllamaCliAdapter;
use heiwa_provider::providers::claude_code::ClaudeCodeCliAdapter;
use heiwa_repl::{parse_input, render_footer, ReplCommand, TelemetryState};
use std::io::{self, Write, IsTerminal};

fn canonical_surface_id(provider: &str) -> &str {
    match provider {
        "claude-code" => "claude",
        "google-gemini-cli" => "gemini",
        "google-antigravity" => "antigravity",
        _ => provider,
    }
}

fn provider_supports_loop_adapter(provider: &str) -> bool {
    matches!(canonical_surface_id(provider), "claude" | "ollama")
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RoutePreference {
    Auto,
    LocalOnly,
    RemoteOnly,
}

// ---------------------------------------------------------------------------
// Shared session state — used by both plain REPL and cockpit controller
// ---------------------------------------------------------------------------

struct SessionPins {
    pinned_provider: Option<String>,
    pinned_model: Option<String>,
    route_preference: RoutePreference,
    current_provider: String,
    current_model: String,
}

impl SessionPins {
    fn new() -> Self {
        Self {
            pinned_provider: None,
            pinned_model: None,
            route_preference: RoutePreference::Auto,
            current_provider: String::new(),
            current_model: String::new(),
        }
    }
}

/// Result of successfully routing a task to a model.
struct RouteResult {
    adapter: Arc<dyn ProviderAdapter>,
    model_id: String,
    provider: String,
    provider_model_id: String,
    routing_metadata: String,
    intent_key: String,
    request_id: String,
    turn_started_at: String,
}

/// Outcome of the routing pipeline.
enum RouteOutcome {
    /// Task routed to a model, ready to stream.
    Routed(RouteResult),
    /// DREX returned a deterministic response (no model needed).
    Deterministic(String),
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();

    let is_tty = std::io::stdout().is_terminal();
    let is_plain = args.iter().any(|arg| arg == "--plain");
    let use_cockpit = is_tty && !is_plain;

    if args.len() < 2 || (args.len() == 2 && args[1] == "--plain") {
        run_repl(use_cockpit).await?;
        return Ok(());
    }

    match args[1].as_str() {
        "install" => {
            heiwa_install::run_install()?;
            println!("Registering device...");
            let stdb_client = attempt_stdb_connection().await;
            register_current_device(&stdb_client).await?;
        }
        "repair" => {
            heiwa_install::run_install()?;
            println!("Rebuilt Heiwa runtime root.");
        }
        "login" => {
            if args.len() < 3 {
                println!("Usage: heiwa login [token]");
            } else {
                let identity = heiwa_provider::login_heiwa(&args[2])?;
                println!("Successfully logged in as {} ({})", identity.display_name.as_deref().unwrap_or_default(), identity.user_id);
                
                // Write structured runtime connection state with default STDB endpoint.
                let conn_path = RuntimePaths::discover().connection();
                let conn_json = serde_json::json!({
                    "url": "https://maincloud.spacetimedb.com",
                    "database": "heiwaproductiondb",
                    "token": ""
                });
                if let Some(parent) = conn_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
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
            let paths = RuntimePaths::discover();
            println!("Heiwa Doctor Report:");
            println!("  Runtime root: {}", paths.root().display());
            println!(
                "  Default mode: {}",
                if paths.concise_mode().exists() {
                    "concise"
                } else {
                    "missing"
                }
            );
            println!("  Ownership: providers=inference/auth, heiwa=sessions/sandboxes");
            println!("  Projections:");
            println!(
                "    Codex: {}",
                if paths.root().join("generated/codex/config.toml").exists() {
                    "generated"
                } else {
                    "missing"
                }
            );
            println!(
                "    Claude: {}",
                if paths.root().join("generated/claude/settings.json").exists() {
                    "generated"
                } else {
                    "missing"
                }
            );
            println!(
                "    Gemini: {}",
                if paths.root().join("generated/gemini/settings.json").exists() {
                    "generated"
                } else {
                    "missing"
                }
            );
            println!(
                "    Antigravity: {}",
                if paths.root().join("generated/antigravity/settings.json").exists() {
                    "generated"
                } else {
                    "missing"
                }
            );
            println!();
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
                                let surface_provider = canonical_surface_id(&a.provider);
                                println!(
                                    "  {:<20} {:<12} ({}) [{:?}] — {} models",
                                    a.account_id, surface_provider, a.credential.kind_label(),
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
                    let surface_provider = canonical_surface_id(&account.provider);
                    let loop_cap = if provider_supports_loop_adapter(&account.provider) { " [loop]" } else { "" };
                    println!(
                        "  {:<20} {} ({}) [{:?}] — {} model{}{}",
                        account.account_id,
                        surface_provider,
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
                    let in_registry = registry.accounts.iter().any(|a| canonical_surface_id(&a.provider) == p);
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
                        println!(
                            "\n  {} ({}) [rate: {}]",
                            canonical_surface_id(&m.provider),
                            kind,
                            m.rate_group
                        );
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
            let use_cockpit = std::io::stdout().is_terminal();
            run_repl(use_cockpit).await?;
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
    println!("  repair                        Rebuild Heiwa runtime files and projections");
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
        let surface_provider = canonical_surface_id(&account.provider);
        let models_json = serde_json::to_string(&account.models).unwrap_or_else(|_| "[]".to_string());
        stdb_client.sync_provider_status(
            &account.account_id,
            surface_provider,
            &device_id,
            account.credential.kind_label(),
            &account.account_id, // local_handle_ref
            &format!("{:?}", account.status),
            None,
            None,
            &models_json,
        )?;
        println!("  Synced provider {} status: {:?}", surface_provider, account.status);
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
            let surface_provider = canonical_surface_id(&m.provider).to_string();
            let mut strengths = vec!["chat"];
            if m.supports_tools { strengths.push("tool_use"); }
            if m.supports_vision { strengths.push("vision"); }
            if m.capability_class >= 4 { strengths.push("advanced_coding"); }

            heiwa_bindings::ModelTier {
                id: 0,
                model_id: m.model_id.clone(),
                provider_model_id: m.provider_model_id.clone(),
                provider: surface_provider,
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

async fn run_repl(use_cockpit: bool) -> Result<()> {
    if !use_cockpit {
        println!("Heiwa Interactive Shell");
        println!("Type /help for commands, !command for shell escape, or enter a task.");
        println!();
    }

    let stdb_client = attempt_stdb_connection().await;
    if !use_cockpit {
        if stdb_client.is_connected() {
            println!("  Connected to SpacetimeDB");
        } else if heiwa_provider::load_identity().is_some() {
            println!("  SpacetimeDB unreachable — running offline");
        } else {
            println!("  Not logged in — run 'heiwa login' to enable sync");
        }
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

    if model_tiers.is_empty() && !use_cockpit {
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
    let mut pins = SessionPins::new();

    if let Some(first) = model_tiers.first() {
        pins.current_provider = first.provider.clone();
        pins.current_model = first.model_id.clone();
        state.routing.current_provider = pins.current_provider.clone();
        state.routing.current_model = pins.current_model.clone();
    }

    if use_cockpit {
        let (event_tx, event_rx) =
            tokio::sync::mpsc::unbounded_channel::<CockpitEvent>();
        let (cmd_tx, cmd_rx) =
            tokio::sync::mpsc::unbounded_channel::<CockpitCommand>();

        // Spawn the async controller — it owns routing, execution, evidence
        let ctrl_stdb = stdb_client.clone();
        let ctrl_tiers = model_tiers.clone();
        tokio::spawn(async move {
            run_cockpit_controller(cmd_rx, event_tx, ctrl_stdb, ctrl_tiers).await;
        });

        // Run TUI on the main thread (blocking) — it owns terminal I/O
        let stdb_connected = stdb_client.is_connected();
        heiwa_tui::run_cockpit(event_rx, cmd_tx, state, stdb_connected)?;

        return Ok(());
    }

    loop {
        let footer_state = TelemetryState {
            provider: if pins.current_provider.is_empty() { "none".to_string() } else { pins.current_provider.clone() },
            model: if pins.current_model.is_empty() { "none".to_string() } else { pins.current_model.clone() },
            route: current_route_label(pins.route_preference, pins.pinned_provider.as_deref(), pins.pinned_model.as_deref()),
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

                match route_task(&t, &pins, &model_tiers) {
                    Err(msg) => {
                        println!("{}", msg);
                        continue;
                    }
                    Ok(RouteOutcome::Deterministic(response)) => {
                        println!("{}", response);
                        turn_count += 1;
                        continue;
                    }
                    Ok(RouteOutcome::Routed(route)) => {
                        pins.current_provider = route.provider.clone();
                        pins.current_model = route.model_id.clone();
                        record_route_evidence(&stdb_client, &route, &t);

                        state.transcript.push(TranscriptBlock::User(t.clone()));

                        let messages = vec![Message { role: Role::User, content: t }];
                        let (stream_tx, mut stream_rx) = tokio::sync::mpsc::channel(32);
                        let model_id = route.provider_model_id.clone();

                        tokio::spawn({
                            let adapter = route.adapter.clone();
                            async move {
                                if let Err(e) = adapter.send(&model_id, &messages, stream_tx).await {
                                    eprintln!("Adapter error: {}", e);
                                }
                            }
                        });

                        let mut usage = None;
                        let mut full_response = String::new();
                        while let Some(event) = stream_rx.recv().await {
                            match event {
                                StreamEvent::Token(text) => {
                                    print!("{}", text);
                                    io::stdout().flush()?;
                                    full_response.push_str(&text);
                                }
                                StreamEvent::Done(u) => { usage = Some(u); break; }
                                StreamEvent::Error(e) => { eprintln!("\nStream error: {}", e); break; }
                                StreamEvent::ToolUse { name, .. } => {
                                    println!("\n[tool: {}]", name);
                                    state.transcript.push(TranscriptBlock::Tool(name, "executed".to_string()));
                                }
                            }
                        }
                        println!();
                        state.transcript.push(TranscriptBlock::Assistant(full_response));

                        if let Some(ref u) = usage {
                            if u.input_tokens > 0 || u.cost_usd > 0.0 {
                                println!("  [{} in / {} out | ${:.4}]", u.input_tokens, u.output_tokens, u.cost_usd);
                            }
                        }
                        record_run_evidence(&stdb_client, &route, usage.as_ref());
                        turn_count += 1;
                    }
                }
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
                    // Plain-mode specific: re-discovers providers at call time
                    "providers" => {
                        let mut reg = heiwa_provider::AccountRegistry::load();
                        heiwa_provider::detect::auto_discover(&mut reg).await;
                        let tiers = get_live_model_tiers(&reg);
                        for t in tiers {
                            println!("  {} ({}) class:{}", t.model_id, t.provider, t.capability_class);
                        }
                    }
                    // Plain-mode specific: runs loop controller inline
                    "loop" => {
                        let max_turns = args.first().and_then(|s| s.parse::<u32>().ok()).unwrap_or(5);
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
                        let loop_tiers = get_live_model_tiers(&reg);

                        let controller = heiwa_loop::LoopController::new(config, stdb_client.clone(), loop_tiers);
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
                                provider: pins.current_provider.clone(),
                                model: pins.current_model.clone(),
                                route: current_route_label(pins.route_preference, pins.pinned_provider.as_deref(), pins.pinned_model.as_deref()),
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
                    // All other slash commands use shared handler
                    _ => {
                        if let Some(text) = handle_slash(&c, &args, &model_tiers, &mut pins) {
                            println!("{}", text);
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Cockpit controller — async task that processes CockpitCommands
// ---------------------------------------------------------------------------

async fn run_cockpit_controller(
    mut cmd_rx: tokio::sync::mpsc::UnboundedReceiver<CockpitCommand>,
    event_tx: tokio::sync::mpsc::UnboundedSender<CockpitEvent>,
    stdb_client: heiwa_stdb::StdbClient,
    model_tiers: Vec<heiwa_bindings::ModelTier>,
) {
    let mut pins = SessionPins::new();

    if let Some(first) = model_tiers.first() {
        pins.current_provider = first.provider.clone();
        pins.current_model = first.model_id.clone();
    }

    while let Some(cmd) = cmd_rx.recv().await {
        match cmd {
            CockpitCommand::Quit => break,
            CockpitCommand::SubmitInput(input) => {
                let parsed = parse_input(&input);
                match parsed {
                    ReplCommand::Task(t) => {
                        if t.is_empty() { continue; }
                        let _ = event_tx.send(CockpitEvent::StatusUpdate("routing...".into()));

                        match route_task(&t, &pins, &model_tiers) {
                            Err(msg) => {
                                let _ = event_tx.send(CockpitEvent::TranscriptAppend(
                                    TranscriptBlock::Evidence(msg),
                                ));
                                let _ = event_tx.send(CockpitEvent::StatusUpdate("ready".into()));
                                continue;
                            }
                            Ok(RouteOutcome::Deterministic(response)) => {
                                let _ = event_tx.send(CockpitEvent::TranscriptAppend(
                                    TranscriptBlock::Assistant(response),
                                ));
                                let _ = event_tx.send(CockpitEvent::StatusUpdate("ready".into()));
                                continue;
                            }
                            Ok(RouteOutcome::Routed(route)) => {
                                pins.current_provider = route.provider.clone();
                                pins.current_model = route.model_id.clone();

                                let _ = event_tx.send(CockpitEvent::RoutingUpdate(RoutingState {
                                    current_provider: pins.current_provider.clone(),
                                    current_model: pins.current_model.clone(),
                                    mode: current_route_label(
                                        pins.route_preference,
                                        pins.pinned_provider.as_deref(),
                                        pins.pinned_model.as_deref(),
                                    ),
                                    explanation: Some(route.routing_metadata.clone()),
                                }));

                                record_route_evidence(&stdb_client, &route, &t);

                                let _ = event_tx.send(CockpitEvent::StatusUpdate("streaming...".into()));

                                // Stream response
                                let messages = vec![Message { role: Role::User, content: t }];
                                let (stream_tx, mut stream_rx) = tokio::sync::mpsc::channel(32);
                                let model_id = route.provider_model_id.clone();

                                tokio::spawn({
                                    let adapter = route.adapter.clone();
                                    let err_tx = event_tx.clone();
                                    async move {
                                        if let Err(e) = adapter.send(&model_id, &messages, stream_tx).await {
                                            let _ = err_tx.send(CockpitEvent::StreamError(
                                                format!("adapter error: {}", e),
                                            ));
                                        }
                                    }
                                });

                                let mut usage = None;
                                while let Some(ev) = stream_rx.recv().await {
                                    match ev {
                                        StreamEvent::Token(text) => {
                                            let _ = event_tx.send(CockpitEvent::StreamToken(text));
                                        }
                                        StreamEvent::Done(u) => { usage = Some(u); break; }
                                        StreamEvent::Error(e) => {
                                            let _ = event_tx.send(CockpitEvent::StreamError(e));
                                            break;
                                        }
                                        StreamEvent::ToolUse { name, .. } => {
                                            let _ = event_tx.send(CockpitEvent::TranscriptAppend(
                                                TranscriptBlock::Tool(name, "executed".to_string()),
                                            ));
                                        }
                                    }
                                }

                                if let Some(ref u) = usage {
                                    let _ = event_tx.send(CockpitEvent::StreamDone {
                                        tokens_in: u.input_tokens as i64,
                                        tokens_out: u.output_tokens as i64,
                                        cost: u.cost_usd,
                                    });
                                } else {
                                    let _ = event_tx.send(CockpitEvent::StreamDone {
                                        tokens_in: 0, tokens_out: 0, cost: 0.0,
                                    });
                                }
                                record_run_evidence(&stdb_client, &route, usage.as_ref());
                                let _ = event_tx.send(CockpitEvent::StatusUpdate("ready".into()));
                            }
                        }
                    }
                    ReplCommand::Shell(s) => {
                        let output = std::process::Command::new("sh")
                            .arg("-c")
                            .arg(&s)
                            .output();
                        match output {
                            Ok(o) => {
                                let stdout_str = String::from_utf8_lossy(&o.stdout).to_string();
                                let stderr_str = String::from_utf8_lossy(&o.stderr).to_string();
                                let combined = if stderr_str.is_empty() {
                                    stdout_str
                                } else {
                                    format!("{}\n{}", stdout_str, stderr_str)
                                };
                                let _ = event_tx.send(CockpitEvent::TranscriptAppend(
                                    TranscriptBlock::Tool(format!("shell: {}", s), combined),
                                ));
                            }
                            Err(e) => {
                                let _ = event_tx.send(CockpitEvent::TranscriptAppend(
                                    TranscriptBlock::Evidence(format!("shell error: {}", e)),
                                ));
                            }
                        }
                    }
                    ReplCommand::Slash(c, args) => {
                        let msg = handle_slash(&c, &args, &model_tiers, &mut pins);
                        if let Some(text) = msg {
                            let _ = event_tx.send(CockpitEvent::TranscriptAppend(
                                TranscriptBlock::Evidence(text),
                            ));
                        }
                        let _ = event_tx.send(CockpitEvent::RoutingUpdate(RoutingState {
                            current_provider: pins.current_provider.clone(),
                            current_model: pins.current_model.clone(),
                            mode: current_route_label(
                                pins.route_preference,
                                pins.pinned_provider.as_deref(),
                                pins.pinned_model.as_deref(),
                            ),
                            explanation: None,
                        }));
                        let _ = event_tx.send(CockpitEvent::StatusUpdate("ready".into()));
                    }
                }
            }
        }
    }
}

/// Handle slash commands, returning text to display. Shared by both modes.
fn handle_slash(
    cmd: &str,
    args: &[String],
    model_tiers: &[heiwa_bindings::ModelTier],
    pins: &mut SessionPins,
) -> Option<String> {
    match cmd {
        "help" => Some(
            "commands: /provider [name|auto] /providers /model [name|auto] /models /route [auto|local|remote] /status /clear /loop /exit"
                .to_string(),
        ),
        "providers" => {
            if model_tiers.is_empty() {
                Some("no loop-capable providers available".into())
            } else {
                let providers = available_providers(model_tiers);
                let list: Vec<String> = providers
                    .iter()
                    .map(|p| {
                        let count = model_tiers.iter().filter(|t| &t.provider == p).count();
                        format!("{} ({} models)", p, count)
                    })
                    .collect();
                Some(list.join("\n"))
            }
        }
        "auth" => Some("manage auth via 'heiwa auth' in the terminal".into()),
        "loop" => Some("loop: use '/loop [turns] [objective]' in plain mode or 'heiwa loop'".into()),
        "provider" => {
            let available = available_providers(model_tiers);
            match args.first().map(|s| s.as_str()) {
                None => {
                    let active = pins.pinned_provider.as_deref().unwrap_or("auto");
                    Some(format!(
                        "provider: {} | available: {}",
                        active,
                        available.join(", ")
                    ))
                }
                Some("auto") | Some("clear") => {
                    pins.pinned_provider = None;
                    pins.pinned_model = None;
                    Some("provider routing reset to auto".into())
                }
                Some(p) => {
                    if available.iter().any(|x| x == p) {
                        pins.pinned_provider = Some(p.to_string());
                        if let Some(model) = pins.pinned_model.as_ref() {
                            let matches = model_tiers
                                .iter()
                                .any(|t| t.model_id == *model && t.provider == p);
                            if !matches {
                                pins.pinned_model = None;
                            }
                        }
                        Some(format!("pinned provider to {}", p))
                    } else {
                        Some(format!("unknown provider '{}'", p))
                    }
                }
            }
        }
        "model" => match args.first().map(|s| s.as_str()) {
            None => {
                let active = pins.pinned_model.as_deref().unwrap_or("auto");
                let list: Vec<String> = model_tiers
                    .iter()
                    .map(|t| format!("{} ({})", t.model_id, t.provider))
                    .collect();
                Some(format!("model: {} | available: {}", active, list.join(", ")))
            }
            Some("auto") | Some("clear") => {
                pins.pinned_model = None;
                Some("model routing reset to auto".into())
            }
            Some(m) => {
                if let Some(tier) = model_tiers
                    .iter()
                    .find(|t| t.model_id == m || t.provider_model_id == m)
                {
                    pins.pinned_model = Some(tier.model_id.clone());
                    pins.pinned_provider = Some(tier.provider.clone());
                    Some(format!(
                        "pinned model to {} ({})",
                        tier.model_id, tier.provider
                    ))
                } else {
                    Some(format!("unknown model '{}'", m))
                }
            }
        },
        "models" => {
            if model_tiers.is_empty() {
                Some("no loop-capable models available".into())
            } else {
                let list: Vec<String> = model_tiers
                    .iter()
                    .map(|t| {
                        format!(
                            "{} ({}) class:{}",
                            t.model_id, t.provider, t.capability_class
                        )
                    })
                    .collect();
                Some(list.join("\n"))
            }
        }
        "route" => match args.first().map(|s| s.as_str()) {
            None => Some(format!(
                "route: {} | options: auto, local, remote",
                route_preference_label(pins.route_preference)
            )),
            Some("auto") => {
                pins.route_preference = RoutePreference::Auto;
                Some("route preference: auto".into())
            }
            Some("local") => {
                pins.route_preference = RoutePreference::LocalOnly;
                Some("route preference: local-only".into())
            }
            Some("remote") => {
                pins.route_preference = RoutePreference::RemoteOnly;
                Some("route preference: remote-only".into())
            }
            Some(other) => Some(format!("unknown route preference '{}'", other)),
        },
        "status" => Some(format!(
            "provider: {} | model: {} | route: {} | pinned_provider: {} | pinned_model: {}",
            if pins.current_provider.is_empty() {
                "none"
            } else {
                &pins.current_provider
            },
            if pins.current_model.is_empty() {
                "none"
            } else {
                &pins.current_model
            },
            route_preference_label(pins.route_preference),
            pins.pinned_provider.as_deref().unwrap_or("auto"),
            pins.pinned_model.as_deref().unwrap_or("auto"),
        )),
        "clear" => {
            pins.pinned_provider = None;
            pins.pinned_model = None;
            pins.route_preference = RoutePreference::Auto;
            Some("cleared route, provider, and model pins".into())
        }
        "exit" | "quit" => None,
        _ => Some(format!("unknown command: /{}", cmd)),
    }
}

// ---------------------------------------------------------------------------
// Shared execution core — used by both plain REPL and cockpit controller
// ---------------------------------------------------------------------------

/// Providers that have a working adapter in `resolve_adapter()`.
const SUPPORTED_ADAPTER_PROVIDERS: &[&str] = &["ollama", "claude"];

/// Returns true if the provider has a working adapter in this binary.
fn has_adapter(provider: &str) -> bool {
    SUPPORTED_ADAPTER_PROVIDERS.contains(&canonical_surface_id(provider))
}

/// Route a task through DREX, returning the adapter + metadata needed to stream.
fn route_task(
    task: &str,
    pins: &SessionPins,
    model_tiers: &[heiwa_bindings::ModelTier],
) -> Result<RouteOutcome, String> {
    let turn_request = parse_turn_intent(task);
    let (provider_pin, model_pin) = match (turn_request.provider_pin, turn_request.model_pin) {
        (Some(p), Some(m)) => (Some(p), Some(m)),
        (Some(p), None) => (Some(p), None),
        _ => (None, None),
    };

    let final_provider_pin = provider_pin.as_deref().or(pins.pinned_provider.as_deref());
    let final_model_pin = model_pin.as_deref().or(pins.pinned_model.as_deref());

    // Filter to providers with working adapters before DREX ever sees them.
    let adapter_capable: Vec<heiwa_bindings::ModelTier> = model_tiers
        .iter()
        .filter(|t| has_adapter(&t.provider))
        .cloned()
        .collect();

    if adapter_capable.is_empty() {
        return Err(format!(
            "No models with working adapters. Supported providers: {}.",
            SUPPORTED_ADAPTER_PROVIDERS.join(", "),
        ));
    }

    let routed_tiers = filtered_model_tiers(&adapter_capable, pins.route_preference, final_provider_pin, final_model_pin);

    if routed_tiers.is_empty() {
        let reason = if final_model_pin.is_some() {
            format!("Model '{}' not available.", final_model_pin.unwrap())
        } else if final_provider_pin.is_some() {
            let supported: Vec<&str> = adapter_capable.iter().map(|t| t.provider.as_str()).collect::<std::collections::HashSet<_>>().into_iter().collect();
            format!(
                "Provider '{}' not available. Supported: {}.",
                final_provider_pin.unwrap(),
                supported.join(", "),
            )
        } else {
            "No models available.".to_string()
        };
        return Err(format!("Routing failed: {}", reason));
    }

    let ingress = DrexIngress {
        intent: turn_request.intent.as_drex_key().to_string(),
        risk: "low".to_string(),
        raw_text: task.to_string(),
        privacy: "standard".to_string(),
        runtime: runtime_for_route_preference(pins.route_preference).to_string(),
        available_vram_mb: 8192,
        required_context_tokens: 1024,
    };

    let policy = default_policy();
    let preflight = preflight_execution(&ingress, &routed_tiers, &policy);

    match preflight.execution_mode {
        ExecutionMode::Deterministic | ExecutionMode::Clarify => {
            let response = preflight.response_text.unwrap_or_default();
            return Ok(RouteOutcome::Deterministic(response));
        }
        _ => {}
    }

    let effective_route_preference = if pins.route_preference == RoutePreference::Auto {
        match preflight.execution_mode {
            ExecutionMode::LocalModel => RoutePreference::LocalOnly,
            ExecutionMode::RemoteModel => RoutePreference::RemoteOnly,
            _ => RoutePreference::Auto,
        }
    } else {
        pins.route_preference
    };

    let effective_tiers = filtered_model_tiers(
        &routed_tiers,
        effective_route_preference,
        final_provider_pin,
        final_model_pin,
    );

    if effective_tiers.is_empty() {
        return Err("Routing failed: model stack unavailable for this execution mode.".to_string());
    }

    let route = plan_route(&ingress, &effective_tiers, &policy)
        .map_err(|e| format!("Routing failed: {}", e))?;

    let selected = route
        .selected_model
        .as_ref()
        .ok_or_else(|| "No model matched for this task.".to_string())?;

    let adapter = resolve_adapter(&selected.provider, &selected.model_id)?;

    Ok(RouteOutcome::Routed(RouteResult {
        adapter,
        model_id: selected.model_id.clone(),
        provider: selected.provider.clone(),
        provider_model_id: selected.provider_model_id.clone(),
        routing_metadata: route.routing_metadata,
        intent_key: turn_request.intent.as_drex_key().to_string(),
        request_id: uuid::Uuid::new_v4().to_string(),
        turn_started_at: Utc::now().to_rfc3339(),
    }))
}

/// Resolve a provider adapter by name.
fn resolve_adapter(provider: &str, model_id: &str) -> Result<Arc<dyn ProviderAdapter>, String> {
    match canonical_surface_id(provider) {
        "ollama" => Ok(Arc::new(OllamaCliAdapter::with_model(model_id))),
        "claude" => Ok(Arc::new(ClaudeCodeCliAdapter::new())),
        _ => Err(format!("No adapter for provider '{}' yet.", provider)),
    }
}

/// Record a DREX route decision in SpacetimeDB.
fn record_route_evidence(
    stdb: &heiwa_stdb::StdbClient,
    route: &RouteResult,
    task: &str,
) {
    let _ = stdb.record_route_decision(
        &route.request_id,
        &route.request_id,
        task,
        &route.intent_key,
        "low",
        "standard",
        &route.provider,
        &route.provider,
        &route.model_id,
        if is_local_provider(&route.provider) { "local" } else { "remote" },
        &route.routing_metadata,
        0.9,
    );
}

/// Record a completed run in SpacetimeDB.
fn record_run_evidence(
    stdb: &heiwa_stdb::StdbClient,
    route: &RouteResult,
    usage: Option<&TokenUsage>,
) {
    let turn_ended_at = Utc::now().to_rfc3339();
    let user_id = heiwa_provider::load_identity()
        .map(|id| id.user_id)
        .unwrap_or_else(|| "anonymous".to_string());

    if let Some(u) = usage {
        let _ = stdb.record_run(
            &format!("run-{}", uuid::Uuid::new_v4()),
            &user_id,
            &route.request_id,
            &route.turn_started_at,
            &turn_ended_at,
            "SUCCESS",
            &route.model_id,
            u.input_tokens as i64,
            u.output_tokens as i64,
            u.cost_usd,
            None,
            None,
            None,
        );
    } else {
        let _ = stdb.record_run(
            &format!("run-{}", uuid::Uuid::new_v4()),
            &user_id,
            &route.request_id,
            &route.turn_started_at,
            &turn_ended_at,
            "COMPLETED_NO_USAGE",
            &route.model_id,
            0,
            0,
            0.0,
            None,
            None,
            None,
        );
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------



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
    matches!(canonical_surface_id(provider), "ollama" | "local" | "vllm" | "litellm")
}

#[cfg(test)]
mod tests {
    use heiwa_provider::{
        AccountRegistry, AccountStatus, Credential, DetectedModel, InventoryTruth, ProviderAccount,
    };
    use heiwa_protocol::{parse_turn_intent, Intent};

    #[test]
    fn greeting_input_defaults_to_chat_intent() {
        assert_eq!(parse_turn_intent("hi").intent, Intent::Chat);
        assert_eq!(parse_turn_intent("hello there").intent, Intent::Chat);
    }

    #[test]
    fn coding_input_uses_build_intent() {
        assert_eq!(parse_turn_intent("refactor this Rust function").intent, Intent::Build);
        assert_eq!(parse_turn_intent("fix the failing cargo test").intent, Intent::Build);
    }

    #[test]
    fn research_input_uses_research_intent() {
        assert_eq!(parse_turn_intent("explain how DREX routing works").intent, Intent::Research);
        assert_eq!(parse_turn_intent("what is the weather like").intent, Intent::Research);
    }

    #[test]
    fn deploy_input_uses_deploy_intent() {
        assert_eq!(parse_turn_intent("deploy this to railway").intent, Intent::Deploy);
        assert_eq!(parse_turn_intent("ship the new release").intent, Intent::Deploy);
    }

    #[test]
    fn strategy_input_uses_strategy_intent() {
        assert_eq!(parse_turn_intent("plan the roadmap for Q3").intent, Intent::Strategy);
        assert_eq!(parse_turn_intent("design the architecture").intent, Intent::Strategy);
    }

    #[test]
    fn audit_input_uses_audit_intent() {
        assert_eq!(parse_turn_intent("review the PR").intent, Intent::Audit);
        assert_eq!(parse_turn_intent("lint the codebase").intent, Intent::Audit);
    }

    #[test]
    fn math_question_does_not_false_positive_to_code() {
        // "what is 3/4?" should not match code just because of `/`
        assert_eq!(parse_turn_intent("what is 3/4?").intent, Intent::Research);
    }

    #[test]
    fn provider_pin_with_using_keyword() {
        let req = parse_turn_intent("using ollama explain the code");
        assert_eq!(req.provider_pin.as_deref(), Some("ollama"));
        assert!(req.model_pin.is_none()); // "explain" is a task starter word
    }

    #[test]
    fn provider_pin_with_keyword() {
        let req = parse_turn_intent("with claude sonnet-4 fix the bug");
        assert_eq!(req.provider_pin.as_deref(), Some("claude"));
        assert_eq!(req.model_pin.as_deref(), Some("sonnet-4"));
    }

    #[test]
    fn has_adapter_filters_known_providers() {
        assert!(super::has_adapter("ollama"));
        assert!(super::has_adapter("claude"));
        assert!(!super::has_adapter("anthropic"));
        assert!(!super::has_adapter("openai"));
    }

    #[test]
    fn provider_aliases_map_to_canonical_surface_ids() {
        assert_eq!(super::canonical_surface_id("claude-code"), "claude");
        assert_eq!(super::canonical_surface_id("google-gemini-cli"), "gemini");
        assert_eq!(super::canonical_surface_id("google-antigravity"), "antigravity");
        assert_eq!(super::canonical_surface_id("codex"), "codex");
    }

    #[test]
    fn live_model_tiers_canonicalize_cli_wrapped_provider_ids() {
        let registry = AccountRegistry {
            accounts: vec![ProviderAccount {
                account_id: "anthropic-cli".to_string(),
                provider: "claude-code".to_string(),
                credential: Credential::OauthCli {
                    binary: "claude".to_string(),
                },
                rate_group: "claude_code".to_string(),
                status: AccountStatus::Connected,
                models: vec![DetectedModel {
                    model_id: "claude/sonnet-4-6".to_string(),
                    provider_model_id: "claude-sonnet-4-6".to_string(),
                    provider: "claude-code".to_string(),
                    account_id: "anthropic-cli".to_string(),
                    rate_group: "claude_code".to_string(),
                    capability_class: 4,
                    context_window: 200_000,
                    supports_streaming: true,
                    supports_tools: true,
                    supports_vision: true,
                    cost_per_1k_input: 0.003,
                    cost_per_1k_output: 0.015,
                    inventory_truth: InventoryTruth::Inferred,
                }],
            }],
        };

        let tiers = super::get_live_model_tiers(&registry);
        assert_eq!(tiers.len(), 1);
        assert_eq!(tiers[0].provider, "claude");
    }
}
