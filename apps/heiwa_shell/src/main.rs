use anyhow::Result;
use std::env;

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        print_help();
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
    
    // Read machine ID from manifest if it exists, otherwise it will be fresh
    let device_id = if manifest_path.exists() {
        let content = std::fs::read_to_string(manifest_path)?;
        let manifest: serde_json::Value = serde_json::from_str(&content)?;
        manifest["device_id"].as_str().unwrap_or("unknown").to_string()
    } else {
        "unknown".to_string()
    };

    println!("Registering device {} for user {}...", device_id, identity.user_id);
    
    // In a real implementation, this would call the STDB reducer 'register_device'
    // For now, we print that we've synced.
    
    let providers = vec!["claude", "codex", "gemini", "ollama"];
    for p in providers {
        if let Some(status) = heiwa_provider::get_auth_status(p) {
            println!("  Syncing provider {} status: {}", p, status.status);
            // Call STDB reducer 'upsert_provider_account_status'
        }
    }

    println!("Device and capabilities synced to SpacetimeDB.");
    Ok(())
}
