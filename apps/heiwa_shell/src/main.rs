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
        }
        "doctor" => {
            let report = heiwa_install::check_installation()?;
            println!("Heiwa Doctor Report:");
            println!("  Rust:   {}", report.rust_version.unwrap_or_else(|| "Not found".to_string()));
            println!("  Node:   {}", report.node_version.unwrap_or_else(|| "Not found".to_string()));
            println!("  Python: {}", report.python_version.unwrap_or_else(|| "Not found".to_string()));
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
    println!("  doctor           Check the status of the Heiwa installation");
    println!("  auth             Manage provider authentication");
    println!("  providers        List available providers and their status");
    println!("  session attach   Attach to a Heiwa session");
    println!("  help             Print this message or the help of the given subcommand(s)");
}
