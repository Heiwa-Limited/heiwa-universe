use heiwa_repl::{parse_input, ReplCommand};

#[test]
fn test_parse_task() {
    let input = "hello world";
    assert_eq!(
        parse_input(input),
        ReplCommand::Task("hello world".to_string())
    );
}

#[test]
fn test_parse_shell() {
    let input = "!ls -la";
    assert_eq!(parse_input(input), ReplCommand::Shell("ls -la".to_string()));
}

#[test]
fn test_parse_slash() {
    let input = "/auth login claude";
    assert_eq!(
        parse_input(input),
        ReplCommand::Slash(
            "auth".to_string(),
            vec!["login".to_string(), "claude".to_string()]
        )
    );
}
