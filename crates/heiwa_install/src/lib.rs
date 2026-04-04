use anyhow::Result;
use std::process::Command;

pub struct DoctorReport {
    pub rust_version: Option<String>,
    pub node_version: Option<String>,
    pub python_version: Option<String>,
    pub claude_installed: bool,
    pub codex_installed: bool,
    pub gemini_installed: bool,
    pub openclaw_installed: bool,
    pub ollama_installed: bool,
}

pub fn check_installation() -> Result<DoctorReport> {
    Ok(DoctorReport {
        rust_version: get_version("rustc", &["--version"]),
        node_version: get_version("node", &["--version"]),
        python_version: get_version("python3", &["--version"]),
        claude_installed: has_command("claude"),
        codex_installed: has_command("codex"),
        gemini_installed: has_command("gemini"),
        openclaw_installed: has_command("openclaw"),
        ollama_installed: has_command("ollama"),
    })
}

pub fn run_install() -> Result<()> {
    println!("Checking prerequisites...");
    let report = check_installation()?;
    
    if report.rust_version.is_none() {
        println!("Rust not found. Please install Rust: https://rustup.rs/");
    }
    
    if report.node_version.is_none() {
        println!("Node.js not found. Please install Node.js: https://nodejs.org/");
    }

    if report.python_version.is_none() {
        println!("Python 3 not found. Please install Python 3.");
    }

    // In a real implementation, this would perform more installation steps
    println!("Installation check complete.");
    
    Ok(())
}

fn get_version(cmd: &str, args: &[&str]) -> Option<String> {
    Command::new(cmd)
        .args(args)
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
            } else {
                None
            }
        })
}

fn has_command(cmd: &str) -> bool {
    Command::new("which")
        .arg(cmd)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}
