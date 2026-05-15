use anyhow::Result;

use crate::cmd;

pub async fn try_handle(args: &[String]) -> Result<bool> {
    match args.get(1).map(String::as_str) {
        Some("life") => {
            cmd::life::run(&args[2..])?;
            Ok(true)
        }
        Some("app") => {
            cmd::app::run(&args[2..]).await?;
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
        Some("mail") => {
            cmd::mail::run(&args[2..])?;
            Ok(true)
        }
        _ => Ok(false),
    }
}
