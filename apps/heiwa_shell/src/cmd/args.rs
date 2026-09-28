//! Argument helpers shared by command modules that follow `heiwa.cli/v1`.

/// Quote one literal argument for a suggested POSIX shell command.
pub(crate) fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// Whether `flag` appears anywhere in `args`.
pub(crate) fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|arg| arg == flag)
}

/// The value after `flag`, unless it is missing or is itself a flag.
pub(crate) fn flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let index = args.iter().position(|arg| arg == flag)?;
    args.get(index + 1)
        .map(String::as_str)
        .filter(|value| !value.starts_with("--"))
}

/// The value after `flag`. `Ok(None)` when the flag is absent; a usage error
/// when it is present without a value, so `--since --once` never silently
/// drops the cursor.
pub(crate) fn optional_value<'a>(
    args: &'a [String],
    flag: &str,
) -> Result<Option<&'a str>, crate::output::CliError> {
    if !has_flag(args, flag) {
        return Ok(None);
    }
    flag_value(args, flag)
        .map(Some)
        .ok_or_else(|| crate::output::CliError::usage(format!("{flag} needs a value")))
}

/// Arguments that are neither flags nor the values of `value_flags`. A value
/// flag never swallows a following flag; consistent with `flag_value`.
pub(crate) fn positionals<'a>(args: &'a [String], value_flags: &[&str]) -> Vec<&'a str> {
    let mut found = Vec::new();
    let mut skip_value = false;
    for arg in args {
        if skip_value {
            skip_value = false;
            if !arg.starts_with("--") {
                continue;
            }
        }
        if value_flags.contains(&arg.as_str()) {
            skip_value = true;
            continue;
        }
        if !arg.starts_with("--") {
            found.push(arg.as_str());
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(raw: &[&str]) -> Vec<String> {
        raw.iter().map(|arg| arg.to_string()).collect()
    }

    #[test]
    fn flags_and_their_values_are_found() {
        let args = argv(&["work-1", "--since", "c-9", "--json"]);
        assert!(has_flag(&args, "--json"));
        assert_eq!(flag_value(&args, "--since"), Some("c-9"));
        assert_eq!(flag_value(&args, "--missing"), None);
    }

    #[test]
    fn a_flag_is_never_taken_as_a_value() {
        let args = argv(&["--since", "--json"]);
        assert_eq!(flag_value(&args, "--since"), None);
    }

    #[test]
    fn positionals_skip_flags_and_the_values_of_value_flags() {
        let args = argv(&["--since", "c-9", "work-1", "--json", "extra"]);
        assert_eq!(positionals(&args, &["--since"]), vec!["work-1", "extra"]);
    }

    #[test]
    fn a_value_flag_never_swallows_the_next_flag() {
        let args = argv(&["--since", "--once", "work-1"]);
        assert_eq!(positionals(&args, &["--since"]), vec!["work-1"]);
    }

    #[test]
    fn a_value_flag_without_a_value_is_a_usage_error() {
        let absent = argv(&["work-1", "--once"]);
        assert_eq!(
            optional_value(&absent, "--since").expect("absent is fine"),
            None
        );

        let present = argv(&["work-1", "--since", "c-9"]);
        assert_eq!(
            optional_value(&present, "--since").expect("value"),
            Some("c-9")
        );

        let missing = argv(&["work-1", "--since", "--once"]);
        let error = optional_value(&missing, "--since").expect_err("missing value");
        assert_eq!(error.code, crate::output::ErrorCode::Usage);
        assert!(error.message.contains("--since"), "{}", error.message);
    }
}
