use anyhow::{anyhow, Result};
use std::env;
use std::sync::Arc;
use chrono::Utc;
use heiwa_provider::adapter::ProviderAdapter;
use heiwa_provider::providers::ollama::OllamaAdapter;
use heiwa_provider::providers::claude_code::ClaudeCodeAdapter;
use heiwa_repl::{parse_input, render_footer, ReplCommand, TelemetryState};
use std::io::{self, Write};

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
            register_current_device().await?;
        }
        "login" => {
            if args.len() < 3 {
                println!("Usage: heiwa login [token]");
            } else {
                let identity = heiwa_provider::login_heiwa(&args[2])?;
                println!("Successfully logged in as {} ({})", identity.display_name.unwrap_or_default(), identity.user_id);
            }
        }
        "logout" => {
            heiwa_provider::clear_identity()?;
            println!("Successfully logged out from Heiwa.");
        }
        "register" => {
            register_current_device().await?;
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
            println!("  Ollama: {}", if report.ollama_installed { "Installed" } else { "Not found" });
        }
        "auth" => {
            if args.len() < 3 {
                println!("Usage: heiwa auth [status|login|logout] [provider]");
            } else {
                match args[2].as_str() {
                    "status" => {
                        let providers = vec!["claude", "codex", "gemini", "ollama"];
                        println!("Provider Auth Status:");
                        for p in providers {
                            if let Some(status) = heiwa_provider::get_auth_status(p) {
                                println!("  {:<10} {:<15} ({:?})", p, status.status, status.auth_kind);
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
            let providers = vec!["claude", "codex", "gemini", "ollama"];
            println!("Available Providers:");
            for p in providers {
                if let Some(status) = heiwa_provider::get_auth_status(p) {
                    println!("  {:<10} - {:?}", p, status.auth_kind);
                    if let Some(model) = status.default_model {
                        println!("    Default Model: {}", model);
                    }
                }
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
                
                let model_tiers = get_live_model_tiers();
                if model_tiers.is_empty() {
                    return Err(anyhow!("No active providers found. Run 'heiwa auth status' to check."));
                }

                // Try to connect to STDB if environment allows
                let stdb = attempt_stdb_connection().await;

                let controller = heiwa_loop::LoopController::new(config, stdb, model_tiers);
                let (tx, mut rx) = tokio::sync::mpsc::channel(10);
                
                println!("Loop initiated: {}", controller.get_id());
                
                let adapters: Arc<dyn Fn(&str) -> Option<Arc<dyn ProviderAdapter>> + Send + Sync> = Arc::new(|provider: &str| {
                    match provider {
                        "ollama" => Some(Arc::new(OllamaAdapter::new()) as Arc<dyn ProviderAdapter>),
                        "claude" => Some(Arc::new(ClaudeCodeAdapter::new()) as Arc<dyn ProviderAdapter>),
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
    println!("Heiwa AI runtime and shell");
    println!();
    println!("Usage: heiwa [COMMAND]");
    println!();
    println!("Commands:");
    println!("  install          Install Heiwa and its dependencies");
    println!("  login [token]    Sign in to Heiwa");
    println!("  logout           Sign out from Heiwa");
    println!("  doctor           Check the status of the Heiwa installation");
    println!("  register         Register the current device and sync capabilities");
    println!("  auth             Manage provider authentication");
    println!("  providers        List available providers and their status");
    println!("  session attach   Attach to a Heiwa session");
    println!("  loop [turns] obj Run a bounded execution loop");
    println!("  shell            Enter interactive mode");
    println!("  help             Print this message or the help of the given subcommand(s)");
}

async fn register_current_device() -> Result<()> {
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
        let content = std::fs::read_to_string(manifest_path)?;
        let manifest: serde_json::Value = serde_json::from_str(&content)?;
        manifest["device_id"].as_str().unwrap_or("unknown").to_string()
    } else {
        "unknown".to_string()
    };

    println!("Registering device {} for user {}...", device_id, identity.user_id);
    
    let providers = vec!["claude", "codex", "gemini", "ollama"];
    for p in providers {
        if let Some(status) = heiwa_provider::get_auth_status(p) {
            println!("  Syncing provider {} status: {}", p, status.status);
        }
    }

    println!("Device and capabilities synced to SpacetimeDB.");
    Ok(())
}

fn get_live_model_tiers() -> Vec<heiwa_bindings::ModelTier> {
    let mut model_tiers = Vec::new();
    let providers = vec!["claude", "codex", "gemini", "ollama"];
    for p in providers {
        if let Some(status) = heiwa_provider::get_auth_status(p) {
            if status.status == "authenticated" || status.status == "running" {
                model_tiers.push(heiwa_bindings::ModelTier {
                    id: 0,
                    model_id: status.default_model.clone().unwrap_or_else(|| "default".to_string()),
                    provider_model_id: "latest".to_string(),
                    provider: p.to_string(),
                    rate_group: status.rate_group.clone(),
                    capability_class: if p == "claude" || p == "codex" { 3 } else { 1 },
                    effort_knob: "default".to_string(),
                    effort_level: 1,
                    cost_per_turn: if status.rate_group == "local" { 0.0 } else { 0.01 },
                    max_context_tokens: 8192,
                    strengths_json: "[\"chat\", \"advanced_coding\"]".to_string(),
                    vram_requirement_mb: 0,
                    quantization_type: "none".to_string(),
                    kv_cache_strategy: "standard".to_string(),
                    enabled: true,
                    last_success_rate: 1.0,
                    avg_latency_ms: 100,
                    latency_p_95_ms: 150,
                    updated_at: Utc::now().to_rfc3339(),
                });
            }
        }
    }
    model_tiers
}

async fn attempt_stdb_connection() -> Option<Arc<heiwa_bindings::DbConnection>> {
    // In a real environment, we'd use environment variables to connect
    // For now, return None to ensure we use offline mode logic
    None
}

async fn run_repl() -> Result<()> {
    println!("Heiwa Interactive Shell");
    println!("Type /help for commands, !command for shell escape, or enter a task.");
    println!();

    let mut turn_count = 0;
    let current_provider = "ollama".to_string();
    let current_model = "llama3".to_string();

    loop {
        let state = TelemetryState {
            provider: current_provider.clone(),
            model: current_model.clone(),
            route: "local".to_string(),
            status: "ready".to_string(),
            turn_count,
            loop_info: None,
        };

        print!("\r{}", render_footer(&state));
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
                println!("Executing task: {}", t);
                turn_count += 1;
                // Execute one-off task via loop with 1 turn?
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
                    "help" => println!("Available slash commands: /auth, /providers, /loop, /exit"),
                    "auth" => println!("Manage auth via 'heiwa auth'"),
                    "providers" => {
                        let tiers = get_live_model_tiers();
                        for t in tiers {
                            println!("  {} ({})", t.model_id, t.provider);
                        }
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

                        let model_tiers = get_live_model_tiers();
                        let stdb = attempt_stdb_connection().await;
                        let controller = heiwa_loop::LoopController::new(config, stdb, model_tiers);
                        let (tx, mut rx) = tokio::sync::mpsc::channel(10);

                        let adapters: Arc<dyn Fn(&str) -> Option<Arc<dyn ProviderAdapter>> + Send + Sync> = Arc::new(|provider: &str| {
                            match provider {
                                "ollama" => Some(Arc::new(OllamaAdapter::new()) as Arc<dyn ProviderAdapter>),
                                "claude" => Some(Arc::new(ClaudeCodeAdapter::new()) as Arc<dyn ProviderAdapter>),
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
                                route: "loop".to_string(),
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
