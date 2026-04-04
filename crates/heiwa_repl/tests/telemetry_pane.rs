use heiwa_repl::{render_footer, TelemetryState};

#[test]
fn test_render_footer() {
    let mut state = TelemetryState {
        provider: "claude".to_string(),
        model: "sonnet".to_string(),
        route: "code".to_string(),
        status: "ready".to_string(),
        turn_count: 5,
        loop_info: None,
    };
    
    let footer = render_footer(&state);
    assert!(footer.contains("ready"));
    assert!(footer.contains("claude"));
    assert!(footer.contains("sonnet"));
    assert!(footer.contains("turns: 5"));

    state.loop_info = Some((3, 10));
    let footer_with_loop = render_footer(&state);
    assert!(footer_with_loop.contains("L: 3/10"));
}
