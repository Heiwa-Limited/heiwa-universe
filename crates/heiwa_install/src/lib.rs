use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MachineManifest {
    pub device_id: String,
    pub hostname: String,
    pub os: String,
    pub arch: String,
    pub installed_at: String,
    pub runtimes: DoctorReport,
}

pub fn get_heiwa_dir() -> PathBuf {
    // Per Seatbelt restrictions and session_context, use the project's temp directory
    let mut path = PathBuf::from("/Users/dmcgregsauce/.gemini/tmp/heiwa-universe");
    path.push("heiwa");
    path
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
    
    let heiwa_dir = get_heiwa_dir();
    fs::create_dir_all(&heiwa_dir)?;
    
    let manifest_path = heiwa_dir.join("machine.json");
    let manifest = MachineManifest {
        device_id: uuid::Uuid::new_v4().to_string(),
        hostname: get_hostname().unwrap_or_else(|| "unknown".to_string()),
        os: env::consts::OS.to_string(),
        arch: env::consts::ARCH.to_string(),
        installed_at: chrono::Utc::now().to_rfc3339(),
        runtimes: report.clone(),
    };
    
    fs::write(&manifest_path, serde_json::to_string_pretty(&manifest)?)?;
    println!("Machine manifest written to {:?}", manifest_path);
    
    if report.rust_version.is_none() {
        println!("Rust not found. Please install Rust: https://rustup.rs/");
    }
    
    if report.node_version.is_none() {
        println!("Node.js not found. Please install Node.js: https://nodejs.org/");
    }

    if report.python_version.is_none() {
        println!("Python 3 not found. Please install Python 3.");
    }

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

fn get_hostname() -> Option<String> {
    Command::new("hostname")
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
