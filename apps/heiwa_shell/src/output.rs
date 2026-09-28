//! The `heiwa` CLI result contract, `heiwa.cli/v1`.
//!
//! One shape for machines and one for people, both from the same data.
//! - With `--json`, a command prints exactly one envelope on stdout.
//! - Without it, the command's human renderer prints to stdout.
//! - An error always prints a readable line on stderr. With `--json` it also
//!   prints an error envelope on stdout, so an agent parses one stream and a
//!   person reads the other.
//!
//! Exit codes follow `docs/design/refs/CLI.md`.

use std::fmt;
use std::io::Write;

use serde_json::{json, Value};

/// Schema of every result envelope.
pub const SCHEMA: &str = "heiwa.cli/v1";
/// Schema of every line in an NDJSON stream such as `heiwa work watch --json`.
pub const STREAM_SCHEMA: &str = "heiwa.cli.stream/v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    /// Anything not classified below (exit 1).
    Failure,
    /// A malformed invocation (exit 2).
    Usage,
    /// The named object does not exist (exit 10).
    NotFound,
}

impl ErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorCode::Failure => "failure",
            ErrorCode::Usage => "usage",
            ErrorCode::NotFound => "not_found",
        }
    }

    pub fn exit_code(self) -> i32 {
        match self {
            ErrorCode::Failure => 1,
            ErrorCode::Usage => 2,
            ErrorCode::NotFound => 10,
        }
    }
}

/// An error a command classifies itself. Anything else is `Failure`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliError {
    pub code: ErrorCode,
    pub message: String,
    pub hint: Option<String>,
}

impl CliError {
    pub fn failure(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Failure, message)
    }

    pub fn usage(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Usage, message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::NotFound, message)
    }

    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            hint: None,
        }
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for CliError {}

/// Whether the caller asked for machine output.
pub fn wants_json(args: &[String]) -> bool {
    args.iter().any(|arg| arg == "--json")
}

pub fn envelope_ok(data: Value, next: &[String]) -> Value {
    json!({ "schema": SCHEMA, "ok": true, "data": data, "next": next })
}

pub fn envelope_err(error: &CliError) -> Value {
    json!({
        "schema": SCHEMA,
        "ok": false,
        "error": {
            "code": error.code.as_str(),
            "message": error.message,
            "hint": error.hint,
        },
        "next": [],
    })
}

/// Classify any error. A `CliError` anywhere in the chain keeps its code;
/// anything else is a `Failure` whose message is the full context chain.
pub fn classify(error: &anyhow::Error) -> CliError {
    if let Some(cli) = error.downcast_ref::<CliError>() {
        return cli.clone();
    }
    if let Some(cli) = error
        .chain()
        .find_map(|cause| cause.downcast_ref::<CliError>())
    {
        return cli.clone();
    }
    CliError::failure(format!("{error:#}"))
}

/// Write one line to stdout and flush. `Ok(false)` means the reader went away
/// (a closed pipe, as in `heiwa work watch … | head -1`). Callers stop quietly
/// rather than letting `println!` panic.
pub fn print_line(line: &str) -> std::io::Result<bool> {
    let mut stdout = std::io::stdout().lock();
    match writeln!(stdout, "{line}").and_then(|()| stdout.flush()) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => Ok(false),
        Err(error) => Err(error),
    }
}

/// Print one success: the envelope with `--json`, otherwise the human view.
pub fn emit(
    json: bool,
    data: Value,
    next: &[String],
    human: impl FnOnce(&Value),
) -> anyhow::Result<()> {
    if json {
        print_line(&envelope_ok(data, next).to_string())?;
    } else {
        human(&data);
    }
    Ok(())
}

/// Print an error per the contract and return the process exit code.
pub fn report_error(error: &anyhow::Error, json: bool) -> i32 {
    let classified = classify(error);
    eprintln!("heiwa: {}", classified.message);
    if let Some(hint) = &classified.hint {
        eprintln!("  hint: {hint}");
    }
    if json {
        // Best effort: the exit code still reports the error if stdout is gone.
        let _ = print_line(&envelope_err(&classified).to_string());
    }
    classified.code.exit_code()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_success_envelope_carries_schema_data_and_next() {
        let envelope = envelope_ok(json!({"work": []}), &["heiwa work list".to_string()]);
        assert_eq!(envelope["schema"], "heiwa.cli/v1");
        assert_eq!(envelope["ok"], true);
        assert_eq!(envelope["data"]["work"], json!([]));
        assert_eq!(envelope["next"][0], "heiwa work list");
    }

    #[test]
    fn an_error_envelope_carries_code_message_and_hint() {
        let error = CliError::not_found("no Work work-1").with_hint("run `heiwa work list`");
        let envelope = envelope_err(&error);
        assert_eq!(envelope["schema"], "heiwa.cli/v1");
        assert_eq!(envelope["ok"], false);
        assert_eq!(envelope["error"]["code"], "not_found");
        assert_eq!(envelope["error"]["message"], "no Work work-1");
        assert_eq!(envelope["error"]["hint"], "run `heiwa work list`");
    }

    #[test]
    fn exit_codes_follow_the_cli_spec() {
        assert_eq!(ErrorCode::Failure.exit_code(), 1);
        assert_eq!(ErrorCode::Usage.exit_code(), 2);
        assert_eq!(ErrorCode::NotFound.exit_code(), 10);
    }

    #[test]
    fn classification_finds_a_cli_error_under_context() {
        let wrapped = anyhow::Error::new(CliError::usage("bad flag")).context("while parsing");
        assert_eq!(classify(&wrapped).code, ErrorCode::Usage);

        let plain = anyhow::anyhow!("disk full");
        let classified = classify(&plain);
        assert_eq!(classified.code, ErrorCode::Failure);
        assert_eq!(classified.message, "disk full");
    }

    #[test]
    fn json_is_requested_only_by_the_flag() {
        let listed: Vec<String> = ["work", "list", "--json"].map(String::from).to_vec();
        let plain: Vec<String> = ["work", "list"].map(String::from).to_vec();
        assert!(wants_json(&listed));
        assert!(!wants_json(&plain));
    }
}
