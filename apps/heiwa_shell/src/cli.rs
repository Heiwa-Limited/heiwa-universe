use anyhow::Result;

use crate::cmd;

pub async fn try_handle(args: &[String]) -> Result<bool> {
    match args.get(1).map(String::as_str) {
        Some("auth") if args.get(2).is_some_and(|command| command == "verify-codex") => {
            verify_codex(&args[3..]).await?;
            Ok(true)
        }
        Some("tools") => {
            cmd::agent_tools::run(&args[2..]).await?;
            Ok(true)
        }
        Some("workflows") => {
            cmd::workflows::run(&args[2..])?;
            Ok(true)
        }
        Some("cost") => {
            cmd::cost::run(&args[2..])?;
            Ok(true)
        }
        Some("life") => {
            cmd::life::run(&args[2..])?;
            Ok(true)
        }
        Some("goal") => {
            cmd::goal::run(&args[2..])?;
            Ok(true)
        }
        Some("compress") => {
            cmd::compress::run(&args[2..]).await?;
            Ok(true)
        }
        Some("app") => {
            cmd::app::run(&args[2..]).await?;
            Ok(true)
        }
        Some("capabilities") => {
            cmd::capabilities::run(&args[2..])?;
            Ok(true)
        }
        Some("workers") => {
            cmd::workers::run(&args[2..])?;
            Ok(true)
        }
        Some("approvals") => {
            cmd::approvals::run(&args[2..])?;
            Ok(true)
        }
        Some("auto") | Some("automations") => {
            cmd::auto::run(&args[2..]).await?;
            Ok(true)
        }
        Some("mesh") => {
            cmd::mesh::run(&args[2..])?;
            Ok(true)
        }
        Some("work") => {
            cmd::work::run(&args[2..])?;
            Ok(true)
        }
        Some("workspace") => {
            cmd::workspace::run(&args[2..])?;
            Ok(true)
        }
        Some("mail") => {
            cmd::mail::run(&args[2..]).await?;
            Ok(true)
        }
        Some("schedule") => {
            cmd::schedule::run(&args[2..])?;
            Ok(true)
        }
        Some("calendar") => {
            cmd::calendar::run(&args[2..]).await?;
            Ok(true)
        }
        Some("reminders") => {
            cmd::reminders::run(&args[2..])?;
            Ok(true)
        }
        Some("finance") => {
            cmd::finance::run(&args[2..]).await?;
            Ok(true)
        }
        Some("connect") => {
            cmd::connectors::run(&args[2..]).await?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

async fn verify_codex(args: &[String]) -> Result<()> {
    use crate::output;
    let params: Vec<&str> = args
        .iter()
        .filter(|arg| arg.as_str() != "--json")
        .map(String::as_str)
        .collect();
    let usage = "heiwa auth verify-codex [model] [--json]";
    if params.as_slice() == ["--help"] || params.as_slice() == ["-h"] {
        return output::emit(
            output::wants_json(args),
            serde_json::json!({"usage":usage,"effect":"Runs a small ChatGPT-plan inference and verifies one host tool; saves an exact-model connection."}),
            &[],
            |_| println!("{usage}"),
        );
    }
    if params.len() > 1 || params.iter().any(|value| value.starts_with('-')) {
        anyhow::bail!(output::CliError::usage(usage));
    }
    let verified = heiwa_provider::connections::connect_codex(params.first().copied()).await?;
    output::emit(
        output::wants_json(args),
        serde_json::json!({"channel":"codex-app-server","model":verified,"verified":true,"proof":"completed host-tool challenge"}),
        &[],
        |_| println!("Verified Codex subscription and host tool execution: {verified}"),
    )
}
