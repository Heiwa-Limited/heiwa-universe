use heiwa_repl::{render_footer, TelemetryState};

#[test]
fn test_render_footer() {
    let state = TelemetryState {
        provider: "claude".to_string(),
        model: "sonnet".to_string(),
        route: "code".to_string(),
        status: "ready".to_string(),
        turn_count: 5,
    };
    
    let footer = render_footer(&state);
    assert!(footer.contains("ready"));
    assert!(footer.contains("claude"));
    assert!(footer.contains("sonnet"));
    assert!(footer.contains("turns: 5"));
}
