//! The `heiwa` command catalog behind `heiwa help` and `heiwa help --json`.
//!
//! One table describes every top-level command. A test scans both
//! dispatchers (`cli::try_handle` and the `match` in `main`), so a command
//! cannot ship without an entry; that gap is how the hand-written help went
//! stale.
//!
//! `v1` lists the invocations whose output follows the `heiwa.cli` contract
//! today. Everything else still prints its own format until it migrates.

use serde_json::{json, Value};

pub struct CommandSpec {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub usage: &'static str,
    pub summary: &'static str,
    pub v1: &'static [&'static str],
}

const fn command(name: &'static str, usage: &'static str, summary: &'static str) -> CommandSpec {
    CommandSpec {
        name,
        aliases: &[],
        usage,
        summary,
        v1: &[],
    }
}

pub const COMMANDS: &[CommandSpec] = &[
    command("install", "install [gh:owner/repo[@ref]]", "Bootstrap Heiwa or install a GitHub plugin"),
    command("setup", "setup [--name <name>]", "First-run setup: identity, provider, readiness"),
    command("whoami", "whoami", "Show this installation's local identity"),
    command("login", "login [token]", "Sign in to Heiwa"),
    command("logout", "logout", "Sign out from Heiwa"),
    command("register", "register", "Register the current device"),
    command("devices", "devices", "Show registered devices"),
    command(
        "doctor",
        "doctor [--ai-ops] [--json]",
        "Check installation, identity, providers, local app reachability",
    ),
    command(
        "auth",
        "auth status|add-key|login|logout",
        "Connected accounts, API keys, and provider CLI login",
    ),
    command("providers", "providers", "List connected accounts and models"),
    command("models", "models", "List all detected models by rate group"),
    command("connect", "connect <connector>", "Connect a provider CLI or life connector"),
    command("ask", "ask <prompt>", "Run one non-interactive turn and print the reply"),
    command("route", "route preview <prompt>", "Preview DREX routing without execution"),
    command("session", "session attach", "Attach to a Heiwa session"),
    command("loop", "loop [turns] <objective>", "Run a bounded execution loop"),
    command("shell", "shell", "Enter interactive mode"),
    command("goal", "goal <subcommand>", "Long-running goals: start, show, step, finish"),
    CommandSpec {
        v1: &["work list", "work create", "work show", "work recover"],
        ..command("work", "work list|create|show|run|recover", "Durable Work on this installation")
    },
    command("workspace", "workspace status|prepare", "Repository hold for a Work"),
    command("workers", "workers heartbeat|status", "Worker liveness registry"),
    command("approvals", "approvals list|show|decide", "Review and decide staged actions"),
    command("receipts", "receipts", "Show run receipt status"),
    command("cost", "cost", "Token and cost totals from local receipts"),
    command("compress", "compress [--text|--file] [--json]", "Compress text with a local model"),
    command(
        "calendar",
        "calendar status|sync|hold|plan",
        "Calendar lanes, local holds, and plan sync",
    ),
    command("schedule", "schedule <text>", "Turn free text into a staged calendar hold"),
    command("mail", "mail status|accounts", "Mail.app metadata-only bridge probe"),
    command(
        "life",
        "life status|today|freshness|approvals|import",
        "Inspect and import life read models",
    ),
    CommandSpec {
        aliases: &["automations"],
        ..command("auto", "auto status|create|tick", "Manage local background automations")
    },
    command("capabilities", "capabilities", "Refresh the local capability inventory"),
    command("mesh", "mesh status|enroll", "Node identity for this machine (no peers yet)"),
    command("app", "app [runtime status]", "Probe local Heiwa.app runtime readiness"),
    CommandSpec {
        aliases: &["--help", "-h"],
        v1: &["help"],
        ..command("help", "help [--json]", "Print this catalog")
    },
    CommandSpec {
        aliases: &["--version", "-V"],
        ..command("version", "version", "Print the heiwa version")
    },
];

/// Exit codes from `docs/design/refs/CLI.md`: (code, name, meaning).
pub const EXIT_CODES: &[(i32, &str, &str)] = &[
    (0, "ok", "success"),
    (1, "failure", "generic failure"),
    (2, "usage", "malformed invocation"),
    (3, "provider_auth", "provider authentication failure"),
    (4, "approval_required", "approval required and not granted"),
    (5, "sandbox_required", "sandbox required and unavailable"),
    (10, "not_found", "the named object does not exist"),
];

pub fn catalog_json() -> Value {
    json!({
        "commands": COMMANDS
            .iter()
            .map(|command| json!({
                "name": command.name,
                "aliases": command.aliases,
                "usage": format!("heiwa {}", command.usage),
                "summary": command.summary,
                "v1": command.v1,
            }))
            .collect::<Vec<_>>(),
        "exit_codes": EXIT_CODES
            .iter()
            .map(|(code, name, meaning)| json!({"code": code, "name": name, "meaning": meaning}))
            .collect::<Vec<_>>(),
        "contract": {
            "result": crate::output::SCHEMA,
            "stream": crate::output::STREAM_SCHEMA,
            "json_flag": "--json prints one result envelope on stdout; diagnostics go to stderr",
        },
    })
}

pub fn print_help() {
    println!("Heiwa — BYOK terminal agent");
    println!();
    println!("Usage: heiwa [COMMAND]");
    println!();
    println!("Commands:");
    for command in COMMANDS {
        println!("  {:<44} {}", command.usage, command.summary);
    }
    println!();
    println!("Machine-readable catalog: heiwa help --json");
}

/// `heiwa help [--json]`.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    crate::output::emit(crate::output::wants_json(args), catalog_json(), &[], |_| {
        print_help()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn catalog_names() -> BTreeSet<&'static str> {
        COMMANDS
            .iter()
            .flat_map(|command| std::iter::once(command.name).chain(command.aliases.iter().copied()))
            .collect()
    }

    /// Names in the `Some("name")` arms of `cli::try_handle`.
    fn cli_dispatch_names() -> BTreeSet<String> {
        include_str!("cli.rs")
            .split("Some(\"")
            .skip(1)
            .filter_map(|rest| rest.split('"').next())
            .map(str::to_string)
            .collect()
    }

    /// Literal arms of the top-level `match args[1].as_str()` in `main`.
    /// Top-level arms sit at exactly eight spaces; nested arms are deeper.
    fn main_dispatch_names() -> BTreeSet<String> {
        let source = include_str!("main.rs");
        let start = source
            .find("match args[1].as_str() {")
            .expect("main dispatch match");
        let mut names = BTreeSet::new();
        for line in source[start..].lines().skip(1) {
            if line.starts_with("        _ =>") {
                break;
            }
            let Some(arm) = line.strip_prefix("        \"") else {
                continue;
            };
            let head = arm.split("=>").next().unwrap_or("");
            for literal in format!("\"{head}").split('"').skip(1).step_by(2) {
                names.insert(literal.to_string());
            }
        }
        names
    }

    #[test]
    fn every_dispatched_command_is_in_the_catalog() {
        let known = catalog_names();
        let dispatched: BTreeSet<String> = cli_dispatch_names()
            .into_iter()
            .chain(main_dispatch_names())
            .collect();
        assert!(dispatched.len() >= 30, "scanner found too little: {dispatched:?}");
        let missing: Vec<&String> = dispatched
            .iter()
            .filter(|name| !known.contains(name.as_str()))
            .collect();
        assert!(missing.is_empty(), "missing from registry::COMMANDS: {missing:?}");
    }

    #[test]
    fn catalog_names_are_unique() {
        let mut seen = BTreeSet::new();
        for command in COMMANDS {
            for name in std::iter::once(command.name).chain(command.aliases.iter().copied()) {
                assert!(seen.insert(name), "duplicate catalog name {name}");
            }
        }
    }

    #[test]
    fn every_v1_invocation_starts_with_its_command() {
        for command in COMMANDS {
            for invocation in command.v1 {
                assert_eq!(
                    invocation.split(' ').next(),
                    Some(command.name),
                    "{invocation} is listed under {}",
                    command.name
                );
            }
        }
    }

    #[test]
    fn the_exit_code_table_agrees_with_error_codes() {
        use crate::output::ErrorCode;
        for code in [ErrorCode::Failure, ErrorCode::Usage, ErrorCode::NotFound] {
            assert!(
                EXIT_CODES
                    .iter()
                    .any(|(number, name, _)| *number == code.exit_code() && *name == code.as_str()),
                "{code:?} missing from EXIT_CODES"
            );
        }
    }

    #[test]
    fn the_catalog_json_lists_commands_and_exit_codes() {
        let catalog = catalog_json();
        let commands = catalog["commands"].as_array().expect("commands");
        assert_eq!(commands.len(), COMMANDS.len());
        let calendar = commands
            .iter()
            .find(|command| command["name"] == "calendar")
            .expect("calendar is catalogued");
        assert_eq!(calendar["usage"], "heiwa calendar status|sync|hold|plan");
        let codes: Vec<i64> = catalog["exit_codes"]
            .as_array()
            .expect("exit codes")
            .iter()
            .map(|code| code["code"].as_i64().expect("code"))
            .collect();
        assert_eq!(codes, vec![0, 1, 2, 3, 4, 5, 10]);
    }
}
