//! Argument helpers shared by command modules that follow `heiwa.cli/v1`.

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

/// Arguments that are neither flags nor the values of `value_flags`.
pub(crate) fn positionals<'a>(args: &'a [String], value_flags: &[&str]) -> Vec<&'a str> {
    let mut found = Vec::new();
    let mut skip_value = false;
    for arg in args {
        if skip_value {
            skip_value = false;
            continue;
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
}
