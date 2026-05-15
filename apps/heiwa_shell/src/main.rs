mod cli;
mod cmd;

use anyhow::{anyhow, Result};
use chrono::Utc;
use heiwa_core::drex::{
    default_policy, plan_route, preflight_execution, DrexIngress, ExecutionMode,
};
use heiwa_protocol::{
    parse_turn_intent, CockpitCommand, CockpitEvent, ExecutionRole, ExecutionScope, Permission,
    PrincipalKind, RiskClass, RoutingState, SessionPrincipal, SessionState, ToolCallReceipt,
    ToolLease, TranscriptBlock,
};
use heiwa_provider::adapter::{Message, ProviderAdapter, Role, StreamEvent, TokenUsage};
use heiwa_provider::providers::claude_code::ClaudeCodeCliAdapter;
use heiwa_provider::providers::codex_cli::CodexCliAdapter;
use heiwa_provider::providers::gemini_cli::GeminiCliAdapter;
use heiwa_provider::providers::ollama::OllamaCliAdapter;
use heiwa_repl::{parse_input, render_footer, ReplCommand, TelemetryState};
use heiwa_shell::agentic;
use std::env;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn provider_supports_loop_adapter(provider: &str) -> bool {
    matches!(provider, "claude" | "codex" | "ollama" | "gemini")
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RoutePreference {
    Auto,
    LocalOnly,
    RemoteOnly,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CockpitMode {
    Direct,
    Agentic,
}

impl CockpitMode {
    fn label(self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::Agentic => "agentic",
        }
    }
}

// ---------------------------------------------------------------------------
// Shared session state — used by both plain REPL and cockpit controller
// ---------------------------------------------------------------------------

struct SessionPins {
    pinned_provider: Option<String>,
    pinned_model: Option<String>,
    route_preference: RoutePreference,
    cockpit_mode: CockpitMode,
    current_provider: String,
    current_model: String,
    principal: SessionPrincipal,
    scope: ExecutionScope,
}

impl SessionPins {
    fn new() -> Self {
        let working_dir = env::current_dir().unwrap_or_else(|_| heiwa_install::get_heiwa_dir());
        let mut scope = ExecutionScope::local_default(working_dir);
        grant_tool_lease(&mut scope, "shell", RiskClass::HostMutating);
        grant_tool_lease(&mut scope, "fs.read", RiskClass::HostSafeReadonly);
        grant_tool_lease(&mut scope, "fs.list", RiskClass::HostSafeReadonly);
        grant_tool_lease(&mut scope, "repo.grep", RiskClass::HostSafeReadonly);
        Self {
            pinned_provider: None,
            pinned_model: None,
            route_preference: RoutePreference::Auto,
            cockpit_mode: CockpitMode::Direct,
            current_provider: String::new(),
            current_model: String::new(),
            principal: SessionPrincipal::new(
                "agent:local-shell",
                PrincipalKind::Agent,
                ExecutionRole::Agent,
            ),
            scope,
        }
    }
}

fn grant_tool_lease(scope: &mut ExecutionScope, name: &str, risk_class: RiskClass) {
    if !scope.tool_leases.iter().any(|lease| lease.name == name) {
        scope.tool_leases.push(ToolLease {
            name: name.to_string(),
            risk_class,
            allowed: true,
        });
    }
}

/// Result of successfully routing a task to a model.
struct RouteResult {
    adapter: Arc<dyn ProviderAdapter>,
    model_id: String,
    provider: String,
    provider_model_id: String,
    rate_group: String,
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

const DEFAULT_SESSION_ID: &str = "default";
const TRANSCRIPT_CHAR_BUDGET: usize = 16_000;

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

    if cli::try_handle(&args).await? {
        return Ok(());
    }

    match args[1].as_str() {
        "install" => match heiwa_install::run_install_target(args.get(2).map(String::as_str))? {
            heiwa_install::InstallOutcome::RuntimeBootstrap => {
                println!("Registering device...");
                let stdb_client = attempt_stdb_connection().await;
                register_current_device(&stdb_client).await?;
            }
            heiwa_install::InstallOutcome::Plugin(plugin) => {
                println!("Plugin installed: {}", plugin.canonical);
                println!("  Path:   {}", plugin.install_dir.display());
                println!("  Remote: {}", plugin.clone_url);
                if let Some(reference) = plugin.source.reference.as_deref() {
                    println!("  Ref:    {}", reference);
                }
            }
        },
        "login" => {
            if args.len() < 3 {
                println!("Usage: heiwa login [token]");
            } else {
                let identity = heiwa_provider::login_heiwa(&args[2])?;
                println!(
                    "Successfully logged in as {} ({})",
                    identity.display_name.as_deref().unwrap_or_default(),
                    identity.user_id
                );

                // Write ~/.heiwa/connection.json with default STDB endpoint
                let heiwa_dir = dirs::home_dir()
                    .map(|h| h.join(".heiwa"))
                    .expect("HOME must be set");
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
                println!(
                    "  ID:       {}",
                    manifest["device_id"].as_str().unwrap_or("unknown")
                );
                println!(
                    "  Hostname: {}",
                    manifest["hostname"].as_str().unwrap_or("unknown")
                );
                println!(
                    "  OS:       {}",
                    manifest["os"].as_str().unwrap_or("unknown")
                );
                println!(
                    "  Arch:     {}",
                    manifest["arch"].as_str().unwrap_or("unknown")
                );
                println!(
                    "  Installed: {}",
                    manifest["installed_at"].as_str().unwrap_or("unknown")
                );

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
            let include_ai_ops = args.iter().any(|arg| arg == "--ai-ops");
            println!("Heiwa Doctor Report:");
            println!(
                "  Rust:   {}",
                report
                    .rust_version
                    .unwrap_or_else(|| "Not found".to_string())
            );
            println!(
                "  Node:   {}",
                report
                    .node_version
                    .unwrap_or_else(|| "Not found".to_string())
            );
            println!(
                "  Python: {}",
                report
                    .python_version
                    .unwrap_or_else(|| "Not found".to_string())
            );
            println!();
            if let Some(identity) = heiwa_provider::load_identity() {
                println!("Heiwa Identity:");
                println!("  User ID: {}", identity.user_id);
                println!(
                    "  Email:   {}",
                    identity.email.unwrap_or_else(|| "N/A".to_string())
                );
            } else {
                println!("Heiwa Identity: Not logged in (run 'heiwa login')");
            }
            println!();
            println!("Providers:");
            println!(
                "  Claude: {}",
                if report.claude_installed {
                    "Installed"
                } else {
                    "Not found"
                }
            );
            println!(
                "  Codex:  {}",
                if report.codex_installed {
                    "Installed"
                } else {
                    "Not found"
                }
            );
            println!(
                "  Gemini: {}",
                if report.gemini_installed {
                    "Installed"
                } else {
                    "Not found"
                }
            );
            println!(
                "  Antigravity: {}",
                if report.antigravity_installed {
                    "Installed"
                } else {
                    "Not found"
                }
            );
            println!(
                "  Ollama: {}",
                if report.ollama_installed {
                    "Installed"
                } else {
                    "Not found"
                }
            );

            if include_ai_ops {
                let ai_ops = heiwa_install::check_ai_ops()?;
                println!();
                println!("AI Ops:");
                print_ai_ops_check("MCP Notion HTTP config", ai_ops.mcp_notion_http);
                print_ai_ops_check("Biome config", ai_ops.biome_configured);
                print_ai_ops_check("npm lint -> Biome", ai_ops.npm_lint_uses_biome);
                print_ai_ops_check("CI Biome gate", ai_ops.ci_lint_uses_biome);
                print_ai_ops_check(
                    "CI Clippy dead_code gate",
                    ai_ops.ci_clippy_dead_code_enforced,
                );
                print_ai_ops_check(
                    "CI unused Rust deps gate",
                    ai_ops.ci_unused_deps_uses_cargo_machete,
                );
                println!(
                    "  Overall: {}",
                    if ai_ops.is_clean() {
                        "Clean"
                    } else {
                        "Needs work"
                    }
                );
            }
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
                                    a.account_id,
                                    a.provider,
                                    a.credential.kind_label(),
                                    a.status,
                                    a.models.len(),
                                );
                            }
                            println!();
                        }
                        // Then show legacy CLI discovery
                        let providers = vec!["claude", "codex", "gemini", "antigravity", "ollama"];
                        println!("CLI Discovery:");
                        for p in providers {
                            if let Some(status) = heiwa_provider::get_auth_status(p) {
                                let loop_capable = if provider_supports_loop_adapter(p) {
                                    " [loop]"
                                } else {
                                    ""
                                };
                                println!(
                                    "  {:<12} {:<20} ({:?}){}",
                                    p, status.status, status.auth_kind, loop_capable
                                );
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
                                &mut registry,
                                provider,
                                api_key,
                                rate_group,
                            ) {
                                Ok(account_id) => {
                                    println!(
                                        "Stored {} API key in Keychain as '{}'",
                                        provider, account_id
                                    );
                                    // Verify key and detect models
                                    print!("Verifying...");
                                    io::stdout().flush()?;
                                    if let Some(account) = registry
                                        .accounts
                                        .iter_mut()
                                        .find(|a| a.account_id == account_id)
                                    {
                                        match heiwa_provider::detect::verify_api_key(account).await
                                        {
                                            Ok(()) => {
                                                println!(
                                                    " {} models available",
                                                    account.models.len()
                                                );
                                                for m in &account.models {
                                                    println!(
                                                        "  {} (class:{})",
                                                        m.model_id, m.capability_class
                                                    );
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
                    let loop_cap = if provider_supports_loop_adapter(&account.provider) {
                        " [loop]"
                    } else {
                        ""
                    };
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
                        let loop_cap = if provider_supports_loop_adapter(p) {
                            " [loop]"
                        } else {
                            ""
                        };
                        unregistered.push(format!(
                            "  {:<20} {} ({:?}){}",
                            p, status.status, status.auth_kind, loop_cap,
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
                        let kind = account
                            .map(|a| a.credential.kind_label())
                            .unwrap_or("unknown");
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
        "route" => {
            run_route_command(&args).await?;
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
                let objective = if args.len() >= 4 {
                    args[3..].join(" ")
                } else {
                    "no objective provided".to_string()
                };

                let identity = heiwa_provider::load_identity()
                    .ok_or_else(|| anyhow!("Not logged in. Please run 'heiwa login' first."))?;

                let intent = if let Some(i) = args.iter().position(|a| a == "--intent") {
                    args[i + 1].clone()
                } else {
                    "code".to_string()
                };
                let risk = if let Some(i) = args.iter().position(|a| a == "--risk") {
                    args[i + 1].clone()
                } else {
                    "low".to_string()
                };
                let privacy = if let Some(i) = args.iter().position(|a| a == "--privacy") {
                    args[i + 1].clone()
                } else {
                    "standard".to_string()
                };

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
                    return Err(anyhow!(
                        "No loop-capable models found. Run 'heiwa providers' to check."
                    ));
                }

                // Try to connect to STDB if environment allows
                let stdb_client = attempt_stdb_connection().await;

                let controller = heiwa_loop::LoopController::new(config, stdb_client, model_tiers);
                let (tx, mut rx) = tokio::sync::mpsc::channel(10);

                println!("Loop initiated: {}", controller.get_id());

                let adapters: Arc<dyn Fn(&str) -> Option<Arc<dyn ProviderAdapter>> + Send + Sync> =
                    Arc::new(|provider: &str| match provider {
                        "ollama" => {
                            Some(Arc::new(OllamaCliAdapter::new()) as Arc<dyn ProviderAdapter>)
                        }
                        "claude" => {
                            Some(Arc::new(ClaudeCodeCliAdapter::new()) as Arc<dyn ProviderAdapter>)
                        }
                        "codex" => {
                            Some(Arc::new(CodexCliAdapter::new()) as Arc<dyn ProviderAdapter>)
                        }
                        "gemini" => {
                            Some(Arc::new(GeminiCliAdapter::new()) as Arc<dyn ProviderAdapter>)
                        }
                        _ => None,
                    });

                let c = controller;
                tokio::spawn(async move {
                    if let Err(e) = c.run(tx, adapters).await {
                        eprintln!("Loop error: {}", e);
                    }
                });

                while let Some(status) = rx.recv().await {
                    println!(
                        "[{}] Turn: {} | Cost: ${:.4}",
                        status.status, status.current_turn, status.total_cost_usd
                    );
                    if status.status == "COMPLETED"
                        || status.status == "CANCELLED"
                        || status.status == "FAILED"
                    {
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
        "--version" | "-V" | "version" => {
            println!("heiwa {}", env!("CARGO_PKG_VERSION"));
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
    println!("  install [gh:owner/repo[@ref]] Bootstrap Heiwa or install a GitHub plugin");
    println!("  login [token]                 Sign in to Heiwa");
    println!("  logout                        Sign out from Heiwa");
    println!("  doctor [--ai-ops]             Check installation and optional AI ops gates");
    println!("  register                      Register the current device");
    println!("  receipts                      Show run receipt status");
    println!("  devices                       Show registered devices");
    println!("  auth status                   Show all connected accounts and CLI discovery");
    println!("  auth add-key <provider> <key> Register an API key for a provider");
    println!("  auth login <provider>         Login to a provider CLI");
    println!("  auth logout <provider>        Logout from a provider CLI");
    println!("  providers                     List connected accounts and models");
    println!("  models                        List all detected models by rate group");
    println!("  life <command>                Inspect/import life readmodel data");
    println!("  app [runtime status]          Probe local Heiwa.app runtime readiness");
    println!("  workers heartbeat             Register local worker liveness");
    println!("  workers status                Show worker registry");
    println!("  approvals list|show|decide    Manage local approval packets");
    println!("  mail status|accounts          Mail.app metadata-only bridge probe");
    println!("  route preview <prompt>        Preview DREX routing without execution");
    println!("  session attach                Attach to a Heiwa session");
    println!("  loop [turns] <objective>      Run a bounded execution loop");
    println!("  shell                         Enter interactive mode");
    println!("  help                          Print this message");
}

async fn run_route_command(args: &[String]) -> Result<()> {
    match args.get(2).map(String::as_str) {
        Some("preview") => {
            let prompt = args
                .get(3..)
                .map(|parts| parts.join(" "))
                .unwrap_or_default();
            if prompt.trim().is_empty() {
                println!("Usage: heiwa route preview <prompt>");
                return Ok(());
            }

            let mut registry = heiwa_provider::AccountRegistry::load();
            heiwa_provider::detect::auto_discover(&mut registry).await;
            let model_tiers = get_live_model_tiers(&registry);
            let pins = SessionPins::new();
            let now_unix = Utc::now().timestamp();
            let quota_ledger = open_default_quota_ledger();

            println!("route preview");
            let quota_lines =
                quota_budget_preview_lines(&model_tiers, quota_ledger.as_ref(), now_unix);
            if !quota_lines.is_empty() {
                println!("  quota:");
                for line in quota_lines {
                    println!("    {line}");
                }
            }

            match route_task_with_quota(
                &prompt,
                &pins,
                &model_tiers,
                quota_ledger.as_ref(),
                now_unix,
            ) {
                Ok(RouteOutcome::Deterministic(response)) => {
                    println!("  mode: deterministic");
                    println!("  response: {}", response);
                }
                Ok(RouteOutcome::Routed(route)) => {
                    let mode = if is_local_provider(&route.provider) {
                        "local_model"
                    } else {
                        "remote_model"
                    };
                    println!("  mode: {}", mode);
                    println!("  intent: {}", route.intent_key);
                    println!("  provider: {}", route.provider);
                    println!("  model: {}", route.model_id);
                    println!("  provider_model: {}", route.provider_model_id);
                    println!("  rate_group: {}", route.rate_group);
                    println!("  metadata: {}", route.routing_metadata);
                }
                Err(error) => {
                    println!("  mode: unavailable");
                    println!("  error: {}", error);
                }
            }
        }
        _ => {
            println!("Usage: heiwa route preview <prompt>");
        }
    }
    Ok(())
}

fn print_ai_ops_check(label: &str, ok: bool) {
    println!("  {:<30} {}", label, if ok { "ok" } else { "missing" });
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
        manifest["device_id"]
            .as_str()
            .unwrap_or("unknown")
            .to_string()
    } else {
        "unknown".to_string()
    };

    let hostname = hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    println!(
        "Registering device {} for user {}...",
        device_id, identity.user_id
    );

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
        let models_json =
            serde_json::to_string(&account.models).unwrap_or_else(|_| "[]".to_string());
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
        println!(
            "  Synced provider {} status: {:?}",
            account.provider, account.status
        );
    }

    if stdb_client.is_connected() {
        println!("Device and capabilities synced to SpacetimeDB.");
    } else {
        println!("Device registered locally (STDB offline — will sync when connected).");
    }
    Ok(())
}

fn get_live_model_tiers(
    registry: &heiwa_provider::AccountRegistry,
) -> Vec<heiwa_bindings::ModelTier> {
    registry
        .all_models()
        .into_iter()
        .filter(|m| provider_supports_loop_adapter(&m.provider))
        .map(|m| {
            let mut strengths = vec!["chat"];
            if m.supports_tools {
                strengths.push("tool_use");
            }
            if m.supports_vision {
                strengths.push("vision");
            }
            if m.capability_class >= 4 {
                strengths.push("advanced_coding");
            }

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

fn print_boot_provider_matrix() {
    // At-a-glance provider sync panel shown on shell boot. This is what a
    // premium CLI looks like — the user sees *what's connected* without
    // having to type anything first.
    const GREEN: &str = "\x1b[32m";
    const YELLOW: &str = "\x1b[33m";
    const DIM: &str = "\x1b[2m";
    const RESET: &str = "\x1b[0m";

    println!("{}Provider sync{}", DIM, RESET);
    let providers = ["ollama", "claude", "gemini", "antigravity", "codex"];
    for pid in providers {
        let Some(acc) = heiwa_provider::get_auth_status(pid) else {
            continue;
        };
        let (glyph, colour) = match acc.status.as_str() {
            "connected" | "running" => ("●", GREEN),
            "installed_unverified" | "installed_stopped" => ("○", YELLOW),
            _ => ("·", DIM),
        };
        println!(
            "  {}{}{} {:<12} {}  {}[{}]{}",
            colour, glyph, RESET, pid, acc.status, DIM, acc.rate_group, RESET
        );
    }
    println!(
        "{}  Use /providers to re-sync, /models to list, /help for commands.{}",
        DIM, RESET
    );
    println!();
}

async fn run_repl(use_cockpit: bool) -> Result<()> {
    if !use_cockpit {
        println!("Heiwa Interactive Shell");
        println!("Type /help for commands, !command for shell escape, or enter a task.");
        println!();
        print_boot_provider_matrix();
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

    let persisted = heiwa_session::load_transcript(DEFAULT_SESSION_ID)
        .unwrap_or_else(|_| heiwa_session::PersistedTranscript::empty(DEFAULT_SESSION_ID));

    let mut state = SessionState {
        session_id: persisted.session_id.clone(),
        transcript: persisted.blocks(),
        routing: RoutingState {
            current_provider: "none".to_string(),
            current_model: "none".to_string(),
            mode: CockpitMode::Direct.label().to_string(),
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
        let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel::<CockpitEvent>();
        let (cmd_tx, cmd_rx) = tokio::sync::mpsc::unbounded_channel::<CockpitCommand>();

        // Spawn the async controller — it owns routing, execution, evidence
        let ctrl_stdb = stdb_client.clone();
        let ctrl_tiers = model_tiers.clone();
        let ctrl_session_id = state.session_id.clone();
        let ctrl_transcript = state.transcript.clone();
        tokio::spawn(async move {
            run_cockpit_controller(
                cmd_rx,
                event_tx,
                ctrl_stdb,
                ctrl_tiers,
                ctrl_session_id,
                ctrl_transcript,
            )
            .await;
        });

        // Run TUI on the main thread (blocking) — it owns terminal I/O
        let stdb_connected = stdb_client.is_connected();
        heiwa_tui::run_cockpit(event_rx, cmd_tx, state, stdb_connected)?;

        return Ok(());
    }

    loop {
        let footer_state = TelemetryState {
            provider: if pins.current_provider.is_empty() {
                "none".to_string()
            } else {
                pins.current_provider.clone()
            },
            model: if pins.current_model.is_empty() {
                "none".to_string()
            } else {
                pins.current_model.clone()
            },
            route: current_route_label(
                pins.route_preference,
                pins.pinned_provider.as_deref(),
                pins.pinned_model.as_deref(),
            ),
            status: "ready".to_string(),
            turn_count,
            loop_info: None,
        };

        print!("\r{}", render_footer(&footer_state));
        print!("\n> ");
        io::stdout().flush()?;

        let mut input = String::new();
        let bytes_read = io::stdin().read_line(&mut input)?;
        if bytes_read == 0 {
            // EOF (Ctrl-D or non-TTY stdin closed). Exit cleanly instead of
            // spinning on zero-byte reads forever.
            println!();
            break;
        }
        let input = input.trim();

        if input == "exit" || input == "quit" {
            break;
        }

        let cmd = parse_input(input);
        match cmd {
            ReplCommand::Task(t) => {
                if t.is_empty() {
                    continue;
                }

                match route_task(&t, &pins, &model_tiers) {
                    Err(msg) => {
                        println!("{}", msg);
                        continue;
                    }
                    Ok(RouteOutcome::Deterministic(response)) => {
                        append_state_block(&mut state, TranscriptBlock::User(t.clone()));
                        append_state_block(
                            &mut state,
                            TranscriptBlock::Assistant(response.clone()),
                        );
                        println!("{}", response);
                        turn_count += 1;
                        continue;
                    }
                    Ok(RouteOutcome::Routed(route)) => {
                        pins.current_provider = route.provider.clone();
                        pins.current_model = route.model_id.clone();
                        record_route_evidence(&stdb_client, &route, &t);

                        let messages = build_messages_from_transcript(&state.transcript, &t, &pins);
                        append_state_block(&mut state, TranscriptBlock::User(t.clone()));
                        let (stream_tx, mut stream_rx) = tokio::sync::mpsc::channel(32);
                        let model_id = route.provider_model_id.clone();

                        tokio::spawn({
                            let adapter = route.adapter.clone();
                            async move {
                                if let Err(e) = adapter.send(&model_id, &messages, stream_tx).await
                                {
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
                                    append_state_block(
                                        &mut state,
                                        TranscriptBlock::Tool(name, "executed".to_string()),
                                    );
                                }
                            }
                        }
                        println!();
                        append_state_block(&mut state, TranscriptBlock::Assistant(full_response));

                        if let Some(ref u) = usage {
                            if u.input_tokens > 0 || u.cost_usd > 0.0 {
                                println!(
                                    "  [{} in / {} out | ${:.4}]",
                                    u.input_tokens, u.output_tokens, u.cost_usd
                                );
                            }
                        }
                        record_run_evidence(&stdb_client, &route, usage.as_ref());
                        turn_count += 1;
                    }
                }
            }
            ReplCommand::Shell(s) => {
                println!("Escaping to shell: {}", s);
                match run_scoped_shell(&s, &pins.scope, &pins.principal) {
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
                            println!(
                                "  {} ({}) class:{}",
                                t.model_id, t.provider, t.capability_class
                            );
                        }
                    }
                    // Plain-mode specific: runs loop controller inline
                    "loop" => {
                        let max_turns = args
                            .first()
                            .and_then(|s| s.parse::<u32>().ok())
                            .unwrap_or(5);
                        let objective = if args.len() > 1 {
                            args[1..].join(" ")
                        } else {
                            "explore context".to_string()
                        };

                        println!("Starting loop: '{}' ({} turns)", objective, max_turns);

                        let identity = heiwa_provider::load_identity().unwrap_or(
                            heiwa_provider::HeiwaIdentity {
                                user_id: "anonymous".to_string(),
                                auth_token: "".to_string(),
                                email: None,
                                display_name: None,
                            },
                        );

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

                        let controller = heiwa_loop::LoopController::new(
                            config,
                            stdb_client.clone(),
                            loop_tiers,
                        );
                        let (tx, mut rx) = tokio::sync::mpsc::channel(10);

                        let adapters: Arc<
                            dyn Fn(&str) -> Option<Arc<dyn ProviderAdapter>> + Send + Sync,
                        > = Arc::new(|provider: &str| match provider {
                            "ollama" => {
                                Some(Arc::new(OllamaCliAdapter::new()) as Arc<dyn ProviderAdapter>)
                            }
                            "claude" => {
                                Some(Arc::new(ClaudeCodeCliAdapter::new())
                                    as Arc<dyn ProviderAdapter>)
                            }
                            "codex" => {
                                Some(Arc::new(CodexCliAdapter::new()) as Arc<dyn ProviderAdapter>)
                            }
                            "gemini" => {
                                Some(Arc::new(GeminiCliAdapter::new()) as Arc<dyn ProviderAdapter>)
                            }
                            _ => None,
                        });

                        tokio::spawn(async move {
                            let _ = controller.run(tx, adapters).await;
                        });

                        while let Some(status) = rx.recv().await {
                            let telemetry = TelemetryState {
                                provider: pins.current_provider.clone(),
                                model: pins.current_model.clone(),
                                route: current_route_label(
                                    pins.route_preference,
                                    pins.pinned_provider.as_deref(),
                                    pins.pinned_model.as_deref(),
                                ),
                                status: status.status.clone(),
                                turn_count,
                                loop_info: Some((status.current_turn, max_turns)),
                            };
                            print!("\r{}\r", render_footer(&telemetry));
                            io::stdout().flush()?;

                            if status.status == "COMPLETED"
                                || status.status == "CANCELLED"
                                || status.status == "FAILED"
                            {
                                println!("\nLoop finished: {}", status.status);
                                break;
                            }
                        }
                    }
                    // All other slash commands use shared handler
                    _ => match handle_slash(&c, &args, &model_tiers, &mut pins) {
                        Some(text) => println!("{}", text),
                        None => break,
                    },
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
    session_id: String,
    mut transcript: Vec<TranscriptBlock>,
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
                        if t.is_empty() {
                            continue;
                        }
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
                                append_controller_block(
                                    &session_id,
                                    &mut transcript,
                                    TranscriptBlock::User(t.clone()),
                                    &event_tx,
                                );
                                append_controller_block(
                                    &session_id,
                                    &mut transcript,
                                    TranscriptBlock::Assistant(response.clone()),
                                    &event_tx,
                                );
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
                                    mode: pins.cockpit_mode.label().to_string(),
                                    explanation: Some(route.routing_metadata.clone()),
                                }));

                                record_route_evidence(&stdb_client, &route, &t);

                                if pins.cockpit_mode == CockpitMode::Agentic {
                                    if route.provider != "ollama" {
                                        let _ = event_tx.send(CockpitEvent::TranscriptAppend(
                                            TranscriptBlock::Evidence(
                                                "agentic mode currently supports ollama only"
                                                    .to_string(),
                                            ),
                                        ));
                                        let _ = event_tx
                                            .send(CockpitEvent::StatusUpdate("ready".into()));
                                        continue;
                                    }

                                    let _ = event_tx.send(CockpitEvent::StatusUpdate(
                                        "agentic: planning tools...".into(),
                                    ));

                                    let mut messages =
                                        build_messages_from_transcript(&transcript, &t, &pins);
                                    messages.insert(
                                        1,
                                        Message {
                                            role: Role::System,
                                            content: agentic::tool_instruction_prompt(),
                                        },
                                    );
                                    append_controller_block(
                                        &session_id,
                                        &mut transcript,
                                        TranscriptBlock::User(t.clone()),
                                        &event_tx,
                                    );

                                    let (first_response, first_usage, first_error) =
                                        collect_adapter_response(
                                            route.adapter.clone(),
                                            route.provider_model_id.clone(),
                                            messages.clone(),
                                        )
                                        .await;

                                    if let Some(error) = first_error {
                                        let _ = event_tx.send(CockpitEvent::StreamError(error));
                                        let _ = event_tx
                                            .send(CockpitEvent::StatusUpdate("ready".into()));
                                        continue;
                                    }

                                    let tool_calls = agentic::parse_tool_calls(&first_response);
                                    if tool_calls.is_empty() {
                                        let _ = event_tx.send(CockpitEvent::StreamToken(
                                            first_response.clone(),
                                        ));
                                        append_controller_block(
                                            &session_id,
                                            &mut transcript,
                                            TranscriptBlock::Assistant(first_response),
                                            &event_tx,
                                        );
                                        send_done_event(&event_tx, first_usage.as_ref());
                                        record_run_evidence(
                                            &stdb_client,
                                            &route,
                                            first_usage.as_ref(),
                                        );
                                        let _ = event_tx
                                            .send(CockpitEvent::StatusUpdate("ready".into()));
                                        continue;
                                    }

                                    let _ = event_tx.send(CockpitEvent::StatusUpdate(
                                        "agentic: running tools...".into(),
                                    ));
                                    match agentic::execute_tool_calls(
                                        pins.scope.clone(),
                                        tool_calls,
                                        &route.provider,
                                        &route.model_id,
                                    )
                                    .await
                                    {
                                        Ok((receipts, tool_entries)) => {
                                            for receipt in &receipts {
                                                record_tool_call_evidence(
                                                    &stdb_client,
                                                    receipt,
                                                    &session_id,
                                                );
                                            }
                                            for entry in &tool_entries {
                                                append_controller_block(
                                                    &session_id,
                                                    &mut transcript,
                                                    TranscriptBlock::Tool(
                                                        entry.name.clone(),
                                                        entry.output.clone(),
                                                    ),
                                                    &event_tx,
                                                );
                                                let _ =
                                                    event_tx.send(CockpitEvent::TranscriptAppend(
                                                        TranscriptBlock::Tool(
                                                            entry.name.clone(),
                                                            entry.output.clone(),
                                                        ),
                                                    ));
                                            }

                                            let _ = event_tx.send(CockpitEvent::StatusUpdate(
                                                "agentic: finalizing...".into(),
                                            ));
                                            messages.push(Message {
                                                role: Role::Assistant,
                                                content: first_response,
                                            });
                                            messages.push(Message {
                                                role: Role::System,
                                                content: agentic::tool_result_prompt(&tool_entries),
                                            });

                                            let (final_response, final_usage, final_error) =
                                                collect_adapter_response(
                                                    route.adapter.clone(),
                                                    route.provider_model_id.clone(),
                                                    messages,
                                                )
                                                .await;
                                            if let Some(error) = final_error {
                                                let _ =
                                                    event_tx.send(CockpitEvent::StreamError(error));
                                            } else {
                                                let _ = event_tx.send(CockpitEvent::StreamToken(
                                                    final_response.clone(),
                                                ));
                                                append_controller_block(
                                                    &session_id,
                                                    &mut transcript,
                                                    TranscriptBlock::Assistant(final_response),
                                                    &event_tx,
                                                );
                                                let usage = merge_usage(first_usage, final_usage);
                                                send_done_event(&event_tx, usage.as_ref());
                                                record_run_evidence(
                                                    &stdb_client,
                                                    &route,
                                                    usage.as_ref(),
                                                );
                                            }
                                        }
                                        Err(error) => {
                                            let _ = event_tx.send(CockpitEvent::StreamError(
                                                format!("tool loop error: {error}"),
                                            ));
                                        }
                                    }
                                    let _ =
                                        event_tx.send(CockpitEvent::StatusUpdate("ready".into()));
                                    continue;
                                }

                                let _ = event_tx
                                    .send(CockpitEvent::StatusUpdate("streaming...".into()));

                                // Stream response
                                let messages =
                                    build_messages_from_transcript(&transcript, &t, &pins);
                                append_controller_block(
                                    &session_id,
                                    &mut transcript,
                                    TranscriptBlock::User(t.clone()),
                                    &event_tx,
                                );
                                let (stream_tx, mut stream_rx) = tokio::sync::mpsc::channel(32);
                                let model_id = route.provider_model_id.clone();

                                tokio::spawn({
                                    let adapter = route.adapter.clone();
                                    let err_tx = event_tx.clone();
                                    async move {
                                        if let Err(e) =
                                            adapter.send(&model_id, &messages, stream_tx).await
                                        {
                                            let _ = err_tx.send(CockpitEvent::StreamError(
                                                format!("adapter error: {}", e),
                                            ));
                                        }
                                    }
                                });

                                let mut usage = None;
                                let mut full_response = String::new();
                                while let Some(ev) = stream_rx.recv().await {
                                    match ev {
                                        StreamEvent::Token(text) => {
                                            full_response.push_str(&text);
                                            let _ = event_tx.send(CockpitEvent::StreamToken(text));
                                        }
                                        StreamEvent::Done(u) => {
                                            usage = Some(u);
                                            break;
                                        }
                                        StreamEvent::Error(e) => {
                                            let _ = event_tx.send(CockpitEvent::StreamError(e));
                                            break;
                                        }
                                        StreamEvent::ToolUse { name, .. } => {
                                            append_controller_block(
                                                &session_id,
                                                &mut transcript,
                                                TranscriptBlock::Tool(
                                                    name.clone(),
                                                    "executed".to_string(),
                                                ),
                                                &event_tx,
                                            );
                                            let _ = event_tx.send(CockpitEvent::TranscriptAppend(
                                                TranscriptBlock::Tool(name, "executed".to_string()),
                                            ));
                                        }
                                    }
                                }

                                append_controller_block(
                                    &session_id,
                                    &mut transcript,
                                    TranscriptBlock::Assistant(full_response),
                                    &event_tx,
                                );

                                if let Some(ref u) = usage {
                                    let _ = event_tx.send(CockpitEvent::StreamDone {
                                        tokens_in: u.input_tokens as i64,
                                        tokens_out: u.output_tokens as i64,
                                        cost: u.cost_usd,
                                    });
                                } else {
                                    let _ = event_tx.send(CockpitEvent::StreamDone {
                                        tokens_in: 0,
                                        tokens_out: 0,
                                        cost: 0.0,
                                    });
                                }
                                record_run_evidence(&stdb_client, &route, usage.as_ref());
                                let _ = event_tx.send(CockpitEvent::StatusUpdate("ready".into()));
                            }
                        }
                    }
                    ReplCommand::Shell(s) => {
                        match run_scoped_shell(&s, &pins.scope, &pins.principal) {
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
                            mode: pins.cockpit_mode.label().to_string(),
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
            "commands: /cwd [folder] /add-dir <folder|glob> /dirs /provider [name|auto] /providers /model [name|auto] /models /route [auto|local|remote] /mode [direct|agentic] /status /clear /loop /exit"
                .to_string(),
        ),
        "cwd" => match args.first() {
            None => Some(format!("cwd: {}", pins.scope.working_dir.display())),
            Some(raw) => match resolve_existing_dir(raw, Some(&pins.scope.working_dir)) {
                Ok(path) => {
                    pins.scope.set_working_dir(path.clone());
                    Some(format!("cwd: {}", path.display()))
                }
                Err(error) => Some(error),
            },
        },
        "add-dir" | "adddir" => {
            if args.is_empty() {
                return Some("usage: /add-dir <folder|glob> [more...]".into());
            }
            let mut added = Vec::new();
            let mut errors = Vec::new();
            for raw in args {
                match expand_dir_arg(raw, Some(&pins.scope.working_dir)) {
                    Ok(paths) if paths.is_empty() => errors.push(format!("no matches: {}", raw)),
                    Ok(paths) => {
                        for path in paths {
                            if pins.scope.add_allowed_dir(path.clone()) {
                                added.push(path);
                            }
                        }
                    }
                    Err(error) => errors.push(error),
                }
            }
            let mut lines = Vec::new();
            if !added.is_empty() {
                lines.push(format!(
                    "added dirs:\n{}",
                    added
                        .iter()
                        .map(|path| format!("  {}", path.display()))
                        .collect::<Vec<_>>()
                        .join("\n")
                ));
            }
            if !errors.is_empty() {
                lines.push(format!("errors:\n  {}", errors.join("\n  ")));
            }
            if lines.is_empty() {
                lines.push("no new dirs".to_string());
            }
            Some(lines.join("\n"))
        }
        "dirs" => Some(format!(
            "cwd: {}\nallowed dirs:\n{}",
            pins.scope.working_dir.display(),
            pins.scope
                .allowed_dirs
                .iter()
                .map(|path| format!("  {}", path.display()))
                .collect::<Vec<_>>()
                .join("\n")
        )),
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
        "mode" => match args.first().map(|s| s.as_str()) {
            None => Some(format!("mode: {}", pins.cockpit_mode.label())),
            Some("direct") => {
                pins.cockpit_mode = CockpitMode::Direct;
                Some("mode: direct".into())
            }
            Some("agentic") => {
                pins.cockpit_mode = CockpitMode::Agentic;
                Some("mode: agentic".into())
            }
            Some(other) => Some(format!("unknown mode '{}'", other)),
        },
        "status" => Some(format!(
            "provider: {} | model: {} | mode: {} | route: {} | pinned_provider: {} | pinned_model: {} | cwd: {} | dirs: {} | sandbox: {:?}",
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
            pins.cockpit_mode.label(),
            route_preference_label(pins.route_preference),
            pins.pinned_provider.as_deref().unwrap_or("auto"),
            pins.pinned_model.as_deref().unwrap_or("auto"),
            pins.scope.working_dir.display(),
            pins.scope.allowed_dirs.len(),
            pins.scope.sandbox_mode,
        )),
        "clear" => {
            pins.pinned_provider = None;
            pins.pinned_model = None;
            pins.route_preference = RoutePreference::Auto;
            pins.cockpit_mode = CockpitMode::Direct;
            Some("cleared route, provider, and model pins".into())
        }
        "exit" | "quit" => None,
        _ => Some(format!("unknown command: /{}", cmd)),
    }
}

fn resolve_existing_dir(raw: &str, base: Option<&Path>) -> Result<PathBuf, String> {
    let path = expand_home(raw);
    let path = if path.is_absolute() {
        path
    } else {
        base.unwrap_or_else(|| Path::new(".")).join(path)
    };
    let canonical = path
        .canonicalize()
        .map_err(|error| format!("invalid directory '{}': {}", raw, error))?;
    if !canonical.is_dir() {
        return Err(format!("not a directory: {}", canonical.display()));
    }
    Ok(canonical)
}

fn expand_dir_arg(raw: &str, base: Option<&Path>) -> Result<Vec<PathBuf>, String> {
    if let Some(parent_raw) = raw.strip_suffix("/*") {
        let parent = resolve_existing_dir(parent_raw, base)?;
        let mut dirs = Vec::new();
        let entries = std::fs::read_dir(&parent)
            .map_err(|error| format!("cannot read '{}': {}", parent.display(), error))?;
        for entry in entries {
            let entry = entry.map_err(|error| error.to_string())?;
            let file_type = entry.file_type().map_err(|error| error.to_string())?;
            if file_type.is_dir() {
                dirs.push(
                    entry
                        .path()
                        .canonicalize()
                        .map_err(|error| error.to_string())?,
                );
            }
        }
        dirs.sort();
        return Ok(dirs);
    }

    resolve_existing_dir(raw, base).map(|path| vec![path])
}

fn expand_home(raw: &str) -> PathBuf {
    if raw == "~" {
        return dirs::home_dir().unwrap_or_else(|| PathBuf::from(raw));
    }
    if let Some(rest) = raw.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(raw)
}

fn run_scoped_shell(
    command: &str,
    scope: &ExecutionScope,
    principal: &SessionPrincipal,
) -> Result<std::process::Output, String> {
    let shell_gate = scope.authorize_tool(principal, "shell", Permission::RunShell);
    if !shell_gate.is_allowed() {
        return Err(shell_gate.reason().to_string());
    }
    if !scope.allows_path(&scope.working_dir) {
        return Err(format!(
            "cwd is outside execution scope: {}",
            scope.working_dir.display()
        ));
    }
    if let Some(path) = first_disallowed_path_reference(command, scope) {
        return Err(format!(
            "shell command references path outside execution scope: {}",
            path.display()
        ));
    }

    std::process::Command::new("sh")
        .arg("-c")
        .arg(command)
        .current_dir(&scope.working_dir)
        .output()
        .map_err(|error| error.to_string())
}

fn first_disallowed_path_reference(command: &str, scope: &ExecutionScope) -> Option<PathBuf> {
    command
        .split(|c: char| c.is_whitespace() || matches!(c, ';' | '&' | '|' | '<' | '>' | '(' | ')'))
        .filter_map(normalize_shell_path_token)
        .find(|path| !scope.allows_path(path))
}

fn normalize_shell_path_token(token: &str) -> Option<PathBuf> {
    let token = token
        .trim_matches(|c| matches!(c, '\'' | '"' | '`' | ',' | ':'))
        .trim();
    if token.is_empty() || token.starts_with('-') {
        return None;
    }
    if token.starts_with('/') || token == "~" || token.starts_with("~/") {
        return Some(expand_home(token));
    }
    None
}

fn build_messages_from_transcript(
    transcript: &[TranscriptBlock],
    current_input: &str,
    pins: &SessionPins,
) -> Vec<Message> {
    let mut transcript_messages = Vec::new();
    let mut used_chars = current_input.len();

    for block in transcript.iter().rev() {
        let Some((role, content)) = transcript_block_to_message(block) else {
            continue;
        };
        let content_len = content.len();
        if used_chars + content_len > TRANSCRIPT_CHAR_BUDGET && !transcript_messages.is_empty() {
            break;
        }
        used_chars += content_len;
        transcript_messages.push(Message { role, content });
    }

    transcript_messages.reverse();
    let mut messages = vec![Message {
        role: Role::System,
        content: working_context_prompt(pins),
    }];
    messages.extend(transcript_messages);
    messages.push(Message {
        role: Role::User,
        content: current_input.to_string(),
    });
    messages
}

fn working_context_prompt(pins: &SessionPins) -> String {
    let dirs = pins
        .scope
        .allowed_dirs
        .iter()
        .map(|path| format!("  - {}", path.display()))
        .collect::<Vec<_>>()
        .join("\n");
    let tools = pins
        .scope
        .tool_leases
        .iter()
        .filter(|lease| lease.allowed)
        .map(|lease| format!("  - {} ({})", lease.name, lease.risk_class))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "Heiwa working context:\nprincipal: {} ({:?}/{:?})\ncurrent directory: {}\nallowed directories:\n{}\nsandbox: {:?}\nnetwork: {:?}\nactive tool leases:\n{}",
        pins.principal.id,
        pins.principal.kind,
        pins.principal.role,
        pins.scope.working_dir.display(),
        dirs,
        pins.scope.sandbox_mode,
        pins.scope.network_policy,
        tools
    )
}

fn transcript_block_to_message(block: &TranscriptBlock) -> Option<(Role, String)> {
    match block {
        TranscriptBlock::User(text) => Some((Role::User, text.clone())),
        TranscriptBlock::Assistant(text) => Some((Role::Assistant, text.clone())),
        TranscriptBlock::Tool(name, output) => {
            Some((Role::System, format!("Tool {} output:\n{}", name, output)))
        }
        TranscriptBlock::Evidence(text) => Some((Role::System, format!("Evidence:\n{}", text))),
    }
}

fn append_state_block(state: &mut SessionState, block: TranscriptBlock) {
    if let Err(error) = heiwa_session::append_entry(&state.session_id, block.clone()) {
        eprintln!("Failed to append transcript entry: {}", error);
    }
    state.transcript.push(block);
}

fn append_controller_block(
    session_id: &str,
    transcript: &mut Vec<TranscriptBlock>,
    block: TranscriptBlock,
    event_tx: &tokio::sync::mpsc::UnboundedSender<CockpitEvent>,
) {
    if let Err(error) = heiwa_session::append_entry(session_id, block.clone()) {
        let _ = event_tx.send(CockpitEvent::TranscriptAppend(TranscriptBlock::Evidence(
            format!("transcript persistence error: {}", error),
        )));
    }
    transcript.push(block);
}

// ---------------------------------------------------------------------------
// Shared execution core — used by both plain REPL and cockpit controller
// ---------------------------------------------------------------------------

/// Providers that have a working adapter in `resolve_adapter()`.
const SUPPORTED_ADAPTER_PROVIDERS: &[&str] = &["ollama", "claude", "codex", "gemini"];

/// Returns true if the provider has a working adapter in this binary.
fn has_adapter(provider: &str) -> bool {
    SUPPORTED_ADAPTER_PROVIDERS.contains(&provider)
}

/// Route a task through DREX, returning the adapter + metadata needed to stream.
fn route_task(
    task: &str,
    pins: &SessionPins,
    model_tiers: &[heiwa_bindings::ModelTier],
) -> Result<RouteOutcome, String> {
    route_task_inner(task, pins, model_tiers, None, Utc::now().timestamp(), true)
}

fn route_task_with_quota(
    task: &str,
    pins: &SessionPins,
    model_tiers: &[heiwa_bindings::ModelTier],
    quota_ledger: Option<&heiwa_quota::QuotaLedger>,
    now_unix: i64,
) -> Result<RouteOutcome, String> {
    route_task_inner(task, pins, model_tiers, quota_ledger, now_unix, false)
}

fn route_task_inner(
    task: &str,
    pins: &SessionPins,
    model_tiers: &[heiwa_bindings::ModelTier],
    quota_ledger: Option<&heiwa_quota::QuotaLedger>,
    now_unix: i64,
    use_default_quota_ledger: bool,
) -> Result<RouteOutcome, String> {
    let turn_request = parse_turn_intent(task);
    let (provider_pin, model_pin) = match (turn_request.provider_pin, turn_request.model_pin) {
        (Some(p), Some(m)) => (Some(p), Some(m)),
        (Some(p), None) => (Some(p), None),
        _ => (None, None),
    };

    let final_provider_pin = provider_pin.as_deref().or(pins.pinned_provider.as_deref());
    let final_model_pin = model_pin.as_deref().or(pins.pinned_model.as_deref());

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

    let early_preflight = preflight_execution(&ingress, &[], &policy);
    match early_preflight.execution_mode {
        ExecutionMode::Deterministic | ExecutionMode::Clarify => {
            let response = early_preflight.response_text.unwrap_or_default();
            return Ok(RouteOutcome::Deterministic(response));
        }
        _ => {}
    }

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

    let routed_tiers = filtered_model_tiers(
        &adapter_capable,
        pins.route_preference,
        final_provider_pin,
        final_model_pin,
    );

    if routed_tiers.is_empty() {
        let reason = if final_model_pin.is_some() {
            format!("Model '{}' not available.", final_model_pin.unwrap())
        } else if final_provider_pin.is_some() {
            let supported: Vec<&str> = adapter_capable
                .iter()
                .map(|t| t.provider.as_str())
                .collect::<std::collections::HashSet<_>>()
                .into_iter()
                .collect();
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

    let default_quota_ledger = if quota_ledger.is_none() && use_default_quota_ledger {
        open_default_quota_ledger()
    } else {
        None
    };
    let active_quota_ledger = quota_ledger.or(default_quota_ledger.as_ref());
    let quota_admission = quota_admitted_model_tiers(&routed_tiers, active_quota_ledger, now_unix);
    if quota_admission.admitted.is_empty() {
        let groups = if quota_admission.exhausted_groups.is_empty() {
            "none".to_string()
        } else {
            quota_admission.exhausted_groups.join(", ")
        };
        return Err(format!(
            "Routing failed: quota exhausted for candidate rate groups: {}.",
            groups
        ));
    }

    let preflight = preflight_execution(&ingress, &quota_admission.admitted, &policy);

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
        &quota_admission.admitted,
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
        rate_group: selected.rate_group.clone(),
        routing_metadata: route.routing_metadata,
        intent_key: turn_request.intent.as_drex_key().to_string(),
        request_id: uuid::Uuid::new_v4().to_string(),
        turn_started_at: Utc::now().to_rfc3339(),
    }))
}

/// Resolve a provider adapter by name.
fn resolve_adapter(provider: &str, model_id: &str) -> Result<Arc<dyn ProviderAdapter>, String> {
    match provider {
        "ollama" => Ok(Arc::new(OllamaCliAdapter::with_model(model_id))),
        "claude" => Ok(Arc::new(ClaudeCodeCliAdapter::new())),
        "codex" => Ok(Arc::new(CodexCliAdapter::new())),
        "gemini" => Ok(Arc::new(GeminiCliAdapter::new())),
        _ => Err(format!("No adapter for provider '{}' yet.", provider)),
    }
}

/// Record a DREX route decision in SpacetimeDB.
fn record_route_evidence(stdb: &heiwa_stdb::StdbClient, route: &RouteResult, task: &str) {
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
        if is_local_provider(&route.provider) {
            "local"
        } else {
            "remote"
        },
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
    let run_id = format!("run-{}", uuid::Uuid::new_v4());
    let turn_ended_at = Utc::now();
    let turn_ended_at_rfc3339 = turn_ended_at.to_rfc3339();
    let user_id = heiwa_provider::load_identity()
        .map(|id| id.user_id)
        .unwrap_or_else(|| "anonymous".to_string());

    if let Some(u) = usage {
        let _ = stdb.record_run(
            &run_id,
            &user_id,
            &route.request_id,
            &route.turn_started_at,
            &turn_ended_at_rfc3339,
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
            &run_id,
            &user_id,
            &route.request_id,
            &route.turn_started_at,
            &turn_ended_at_rfc3339,
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

    if let Some(ledger) = open_default_quota_ledger() {
        if let Err(error) =
            record_local_quota_run(&ledger, &run_id, route, usage, turn_ended_at.timestamp())
        {
            debug_log(format_args!("quota ledger write failed: {error}"));
        }
    }
}

const QUOTA_ADMISSION_WINDOW_SECONDS: i64 = 86_400;
const REMOTE_RATE_GROUP_TOKEN_LIMIT: i64 = 200_000;
const LOCAL_QUOTA_WINDOW_SECONDS: i64 = QUOTA_ADMISSION_WINDOW_SECONDS;

fn record_local_quota_run(
    ledger: &heiwa_quota::QuotaLedger,
    run_id: &str,
    route: &RouteResult,
    usage: Option<&TokenUsage>,
    ended_at_unix: i64,
) -> heiwa_quota::Result<()> {
    let (tokens_input, tokens_output, cost, status) = match usage {
        Some(u) => (
            u.input_tokens as i64,
            u.output_tokens as i64,
            u.cost_usd,
            "SUCCESS",
        ),
        None => (0, 0, 0.0, "COMPLETED_NO_USAGE"),
    };
    let started_at_unix = chrono::DateTime::parse_from_rfc3339(&route.turn_started_at)
        .map(|dt| dt.timestamp())
        .unwrap_or(ended_at_unix);
    let tokens = tokens_input.saturating_add(tokens_output);

    ledger.record_use(
        &route.provider,
        &route.rate_group,
        LOCAL_QUOTA_WINDOW_SECONDS,
        tokens,
        1,
        ended_at_unix,
    )?;
    ledger.record_run(&heiwa_quota::RunRecord {
        id: run_id.to_string(),
        provider: route.provider.clone(),
        model_id: route.model_id.clone(),
        started_at_unix,
        ended_at_unix,
        tokens_input,
        tokens_output,
        cost,
        status: status.to_string(),
        meta: serde_json::json!({
            "request_id": route.request_id,
            "intent": route.intent_key,
            "provider_model_id": route.provider_model_id,
            "rate_group": route.rate_group,
            "routing_metadata": route.routing_metadata,
        }),
    })?;
    Ok(())
}

#[derive(Debug)]
struct QuotaAdmission {
    admitted: Vec<heiwa_bindings::ModelTier>,
    exhausted_groups: Vec<String>,
}

fn quota_admitted_model_tiers(
    model_tiers: &[heiwa_bindings::ModelTier],
    ledger: Option<&heiwa_quota::QuotaLedger>,
    now_unix: i64,
) -> QuotaAdmission {
    let mut admitted = Vec::new();
    let mut exhausted_groups = Vec::new();
    let mut seen_exhausted = std::collections::HashSet::new();

    for tier in model_tiers {
        let Some(token_limit) = quota_token_limit_for_tier(tier) else {
            admitted.push(tier.clone());
            continue;
        };
        let Some(ledger) = ledger else {
            let label = format!("{} (ledger unavailable)", quota_group_label(tier));
            if seen_exhausted.insert(label.clone()) {
                exhausted_groups.push(label);
            }
            continue;
        };

        match ledger.remaining_budget(
            &tier.provider,
            &tier.rate_group,
            QUOTA_ADMISSION_WINDOW_SECONDS,
            token_limit,
            now_unix,
        ) {
            Ok(budget) if budget.exhausted => {
                let label = quota_group_label(tier);
                if seen_exhausted.insert(label.clone()) {
                    exhausted_groups.push(label);
                }
            }
            Ok(_) => admitted.push(tier.clone()),
            Err(error) => {
                let label = format!("{} (ledger error)", quota_group_label(tier));
                debug_log(format_args!(
                    "quota admission read failed for {}: {}",
                    quota_group_label(tier),
                    error
                ));
                if seen_exhausted.insert(label.clone()) {
                    exhausted_groups.push(label);
                }
            }
        }
    }

    QuotaAdmission {
        admitted,
        exhausted_groups,
    }
}

fn quota_budget_preview_lines(
    model_tiers: &[heiwa_bindings::ModelTier],
    ledger: Option<&heiwa_quota::QuotaLedger>,
    now_unix: i64,
) -> Vec<String> {
    let mut lines = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for tier in model_tiers
        .iter()
        .filter(|tier| has_adapter(&tier.provider))
    {
        let label = quota_group_label(tier);
        if !seen.insert(label.clone()) {
            continue;
        }

        let Some(token_limit) = quota_token_limit_for_tier(tier) else {
            lines.push(format!("{label}: unmetered"));
            continue;
        };
        let Some(ledger) = ledger else {
            lines.push(format!("{label}: ledger unavailable"));
            continue;
        };

        match ledger.remaining_budget(
            &tier.provider,
            &tier.rate_group,
            QUOTA_ADMISSION_WINDOW_SECONDS,
            token_limit,
            now_unix,
        ) {
            Ok(budget) => lines.push(format!(
                "{}: {}/{} tokens remaining, resets {}",
                label,
                budget.tokens_remaining,
                budget.token_limit,
                format_unix_timestamp(budget.window_resets_at_unix)
            )),
            Err(error) => lines.push(format!("{label}: ledger error ({error})")),
        }
    }

    lines
}

fn quota_token_limit_for_tier(tier: &heiwa_bindings::ModelTier) -> Option<i64> {
    if is_local_provider(&tier.provider) || tier.rate_group == "local" {
        return None;
    }

    env::var("HEIWA_REMOTE_RATE_GROUP_TOKEN_LIMIT")
        .ok()
        .and_then(|raw| raw.parse::<i64>().ok())
        .filter(|limit| *limit > 0)
        .or(Some(REMOTE_RATE_GROUP_TOKEN_LIMIT))
}

fn quota_group_label(tier: &heiwa_bindings::ModelTier) -> String {
    format!("{}/{}", tier.provider, tier.rate_group)
}

fn open_default_quota_ledger() -> Option<heiwa_quota::QuotaLedger> {
    match heiwa_quota::QuotaLedger::open(heiwa_quota::QuotaLedger::default_path()) {
        Ok(ledger) => Some(ledger),
        Err(error) => {
            debug_log(format_args!("quota ledger open failed: {error}"));
            None
        }
    }
}

fn format_unix_timestamp(timestamp: i64) -> String {
    chrono::DateTime::<Utc>::from_timestamp(timestamp, 0)
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_else(|| timestamp.to_string())
}

fn debug_log(args: std::fmt::Arguments<'_>) {
    if env::var_os("HEIWA_DEBUG").is_some() {
        eprintln!("debug: {args}");
    }
}

fn record_tool_call_evidence(
    stdb: &heiwa_stdb::StdbClient,
    receipt: &ToolCallReceipt,
    session_id: &str,
) {
    let user_id = heiwa_provider::load_identity()
        .map(|id| id.user_id)
        .unwrap_or_else(|| "anonymous".to_string());
    let receipt_json = serde_json::to_string(receipt).unwrap_or_else(|_| "{}".to_string());
    let _ = stdb.record_tool_call_receipt(
        &receipt.id,
        &user_id,
        &receipt.call_id,
        Some(session_id.to_string()),
        &receipt.tool_name,
        receipt.status.as_str(),
        &receipt.started_at,
        &receipt.completed_at,
        &receipt_json,
        receipt.error.clone(),
    );
}

async fn collect_adapter_response(
    adapter: Arc<dyn ProviderAdapter>,
    model_id: String,
    messages: Vec<Message>,
) -> (String, Option<TokenUsage>, Option<String>) {
    let (stream_tx, mut stream_rx) = tokio::sync::mpsc::channel(32);
    tokio::spawn(async move {
        if let Err(error) = adapter.send(&model_id, &messages, stream_tx.clone()).await {
            let _ = stream_tx
                .send(StreamEvent::Error(format!("adapter error: {error}")))
                .await;
        }
    });

    let mut full_response = String::new();
    let mut usage = None;
    let mut error = None;
    while let Some(event) = stream_rx.recv().await {
        match event {
            StreamEvent::Token(text) => full_response.push_str(&text),
            StreamEvent::Done(u) => {
                usage = Some(u);
                break;
            }
            StreamEvent::Error(e) => {
                error = Some(e);
                break;
            }
            StreamEvent::ToolUse { name, input } => {
                full_response.push_str(
                    &serde_json::json!({
                        "tool_calls": [{
                            "name": name,
                            "arguments": input,
                        }]
                    })
                    .to_string(),
                );
            }
        }
    }
    (full_response, usage, error)
}

fn merge_usage(first: Option<TokenUsage>, second: Option<TokenUsage>) -> Option<TokenUsage> {
    match (first, second) {
        (None, None) => None,
        (Some(usage), None) | (None, Some(usage)) => Some(usage),
        (Some(a), Some(b)) => Some(TokenUsage {
            input_tokens: a.input_tokens + b.input_tokens,
            output_tokens: a.output_tokens + b.output_tokens,
            cache_read_tokens: a.cache_read_tokens + b.cache_read_tokens,
            cache_write_tokens: a.cache_write_tokens + b.cache_write_tokens,
            cost_usd: a.cost_usd + b.cost_usd,
        }),
    }
}

fn send_done_event(
    event_tx: &tokio::sync::mpsc::UnboundedSender<CockpitEvent>,
    usage: Option<&TokenUsage>,
) {
    let usage = usage.cloned().unwrap_or_default();
    let _ = event_tx.send(CockpitEvent::StreamDone {
        tokens_in: usage.input_tokens as i64,
        tokens_out: usage.output_tokens as i64,
        cost: usage.cost_usd,
    });
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
        .filter(|tier| {
            pinned_provider
                .map(|provider| tier.provider == provider)
                .unwrap_or(true)
        })
        .filter(|tier| {
            pinned_model
                .map(|model| tier.model_id == model)
                .unwrap_or(true)
        })
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

#[cfg(test)]
mod tests {
    use heiwa_protocol::{parse_turn_intent, Intent};

    #[test]
    fn greeting_input_defaults_to_chat_intent() {
        assert_eq!(parse_turn_intent("hi").intent, Intent::Chat);
        assert_eq!(parse_turn_intent("hello there").intent, Intent::Chat);
    }

    #[test]
    fn coding_input_uses_build_intent() {
        assert_eq!(
            parse_turn_intent("refactor this Rust function").intent,
            Intent::Build
        );
        assert_eq!(
            parse_turn_intent("fix the failing cargo test").intent,
            Intent::Build
        );
    }

    #[test]
    fn research_input_uses_research_intent() {
        assert_eq!(
            parse_turn_intent("explain how DREX routing works").intent,
            Intent::Research
        );
        assert_eq!(
            parse_turn_intent("what is the weather like").intent,
            Intent::Research
        );
    }

    #[test]
    fn deploy_input_uses_deploy_intent() {
        assert_eq!(
            parse_turn_intent("deploy this to railway").intent,
            Intent::Deploy
        );
        assert_eq!(
            parse_turn_intent("ship the new release").intent,
            Intent::Deploy
        );
    }

    #[test]
    fn strategy_input_uses_strategy_intent() {
        assert_eq!(
            parse_turn_intent("plan the roadmap for Q3").intent,
            Intent::Strategy
        );
        assert_eq!(
            parse_turn_intent("design the architecture").intent,
            Intent::Strategy
        );
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
        assert!(super::has_adapter("codex"));
        assert!(super::has_adapter("gemini"));
        assert!(!super::has_adapter("anthropic"));
        assert!(!super::has_adapter("openai"));
    }

    #[test]
    fn route_task_handles_greeting_without_models() {
        let pins = super::SessionPins::new();
        let outcome = super::route_task("hi", &pins, &[]).expect("greeting should route");

        match outcome {
            super::RouteOutcome::Deterministic(response) => {
                assert!(
                    response.contains("Ready"),
                    "unexpected response: {response}"
                );
            }
            super::RouteOutcome::Routed(_) => panic!("greeting should not route to a model"),
        }
    }

    #[test]
    fn route_task_skips_remote_group_when_quota_exhausted() {
        let ledger = heiwa_quota::QuotaLedger::open_in_memory().expect("ledger");
        let now = 1_777_000_000;
        ledger
            .record_use(
                "claude",
                "anthropic",
                super::QUOTA_ADMISSION_WINDOW_SECONDS,
                super::REMOTE_RATE_GROUP_TOKEN_LIMIT,
                1,
                now,
            )
            .expect("seed quota");
        let pins = super::SessionPins::new();
        let tiers = vec![
            test_model_tier("claude", "claude-sonnet", "anthropic", 4, 0.20),
            test_model_tier("ollama", "qwen3.5:9b", "local", 3, 0.0),
        ];

        let outcome = super::route_task_with_quota(
            "explain the product strategy tradeoff",
            &pins,
            &tiers,
            Some(&ledger),
            now + 10,
        )
        .expect("route should fall back");

        match outcome {
            super::RouteOutcome::Routed(route) => {
                assert_eq!(route.provider, "ollama");
                assert_eq!(route.rate_group, "local");
            }
            super::RouteOutcome::Deterministic(_) => panic!("strategy task should route"),
        }
    }

    #[test]
    fn quota_admission_fails_remote_closed_without_ledger() {
        let tiers = vec![
            test_model_tier("claude", "claude-sonnet", "anthropic", 4, 0.20),
            test_model_tier("ollama", "qwen3.5:9b", "local", 3, 0.0),
        ];

        let admission = super::quota_admitted_model_tiers(&tiers, None, 1_777_000_000);

        assert_eq!(admission.admitted.len(), 1);
        assert_eq!(admission.admitted[0].provider, "ollama");
        assert!(admission
            .exhausted_groups
            .iter()
            .any(|group| group.contains("claude/anthropic")));
    }

    #[test]
    fn local_quota_record_persists_usage_by_rate_group() {
        let ledger = heiwa_quota::QuotaLedger::open_in_memory().expect("ledger");
        let route = super::RouteResult {
            adapter: std::sync::Arc::new(super::OllamaCliAdapter::new()),
            model_id: "gemma4".to_string(),
            provider: "ollama".to_string(),
            provider_model_id: "gemma4".to_string(),
            rate_group: "local".to_string(),
            routing_metadata: "{\"reason\":\"test\"}".to_string(),
            intent_key: "chat".to_string(),
            request_id: "req-test".to_string(),
            turn_started_at: "2026-05-07T00:00:00Z".to_string(),
        };
        let usage = heiwa_provider::adapter::TokenUsage {
            input_tokens: 12,
            output_tokens: 7,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            cost_usd: 0.03,
        };

        super::record_local_quota_run(&ledger, "run-test", &route, Some(&usage), 1_777_000_000)
            .expect("quota write");

        let quota = ledger
            .get_quota("ollama", "local")
            .expect("quota read")
            .expect("quota row");
        assert_eq!(quota.tokens_used, 19);
        assert_eq!(quota.requests, 1);

        let runs = ledger.recent_runs(1).expect("runs");
        assert_eq!(runs[0].id, "run-test");
        assert_eq!(runs[0].tokens_input, 12);
        assert_eq!(runs[0].tokens_output, 7);
        assert_eq!(runs[0].cost, 0.03);
        assert_eq!(runs[0].meta["request_id"], "req-test");
    }

    fn test_model_tier(
        provider: &str,
        model_id: &str,
        rate_group: &str,
        capability_class: u8,
        cost_per_turn: f64,
    ) -> heiwa_bindings::ModelTier {
        heiwa_bindings::ModelTier {
            id: 0,
            model_id: model_id.to_string(),
            provider_model_id: model_id.to_string(),
            provider: provider.to_string(),
            rate_group: rate_group.to_string(),
            capability_class,
            effort_knob: "default".to_string(),
            effort_level: 1,
            cost_per_turn,
            max_context_tokens: 32_768,
            vram_requirement_mb: 0,
            quantization_type: "none".to_string(),
            kv_cache_strategy: "standard".to_string(),
            strengths_json: serde_json::json!(["chat", "advanced_coding"]).to_string(),
            enabled: true,
            last_success_rate: 1.0,
            avg_latency_ms: 100,
            latency_p_95_ms: 200,
            updated_at: "2026-05-08T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn exit_slash_returns_none() {
        let mut pins = super::SessionPins::new();
        assert!(super::handle_slash("exit", &[], &[], &mut pins).is_none());
        assert!(super::handle_slash("quit", &[], &[], &mut pins).is_none());
    }

    #[test]
    fn cwd_slash_tracks_current_working_directory() {
        let mut pins = super::SessionPins::new();
        let current = std::env::current_dir().unwrap().canonicalize().unwrap();

        let response = super::handle_slash("cwd", &[], &[], &mut pins).unwrap();
        assert!(response.contains(&current.display().to_string()));

        let response = super::handle_slash("cwd", &[".".to_string()], &[], &mut pins).unwrap();
        assert_eq!(pins.scope.working_dir, current);
        assert!(response.contains(&current.display().to_string()));
        assert!(pins.scope.allowed_dirs.iter().any(|path| path == &current));
    }

    #[test]
    fn add_dir_expands_home_children_glob() {
        let mut pins = super::SessionPins::new();
        let response = super::handle_slash("add-dir", &["~/*".to_string()], &[], &mut pins)
            .expect("add-dir should respond");

        assert!(
            response.contains("added dirs") || response.contains("no new dirs"),
            "unexpected response: {response}"
        );
        assert!(!pins.scope.allowed_dirs.is_empty());
    }

    #[test]
    fn model_context_includes_working_dirs() {
        let pins = super::SessionPins::new();
        let messages = super::build_messages_from_transcript(
            &[heiwa_protocol::TranscriptBlock::User("prior".into())],
            "status",
            &pins,
        );
        assert!(matches!(
            messages.first().unwrap().role,
            heiwa_provider::adapter::Role::System
        ));
        assert!(messages
            .first()
            .unwrap()
            .content
            .contains("current directory:"));
        assert_eq!(messages.last().unwrap().content, "status");
    }

    #[test]
    fn mode_slash_switches_between_direct_and_agentic() {
        let mut pins = super::SessionPins::new();
        assert_eq!(pins.cockpit_mode, super::CockpitMode::Direct);

        let response = super::handle_slash("mode", &["agentic".to_string()], &[], &mut pins)
            .expect("mode response");
        assert_eq!(response, "mode: agentic");
        assert_eq!(pins.cockpit_mode, super::CockpitMode::Agentic);

        let response = super::handle_slash("mode", &["direct".to_string()], &[], &mut pins)
            .expect("mode response");
        assert_eq!(response, "mode: direct");
        assert_eq!(pins.cockpit_mode, super::CockpitMode::Direct);
    }

    #[test]
    fn scoped_shell_blocks_paths_outside_execution_scope() {
        let pins = super::SessionPins::new();
        let error =
            super::run_scoped_shell("cat /etc/passwd", &pins.scope, &pins.principal).unwrap_err();
        assert!(error.contains("outside execution scope"));
    }

    #[test]
    fn scoped_shell_denies_viewer_even_with_shell_lease() {
        let mut pins = super::SessionPins::new();
        pins.principal = heiwa_protocol::SessionPrincipal::new(
            "viewer",
            heiwa_protocol::PrincipalKind::HumanUser,
            heiwa_protocol::ExecutionRole::Viewer,
        );

        let error = super::run_scoped_shell("echo ok", &pins.scope, &pins.principal).unwrap_err();
        assert!(error.contains("lacks permission"));
    }
}
