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
            println!("Running install...");
        }
        "doctor" => {
            println!("Running doctor...");
        }
        "auth" => {
            println!("Running auth...");
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
    println!("  session attach   Attach to a Heiwa session");
    println!("  help             Print this message or the help of the given subcommand(s)");
}
