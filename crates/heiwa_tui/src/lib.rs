use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame, Terminal,
};
use std::io;
use tokio::sync::mpsc;

use heiwa_protocol::{CockpitCommand, CockpitEvent, SessionState, TranscriptBlock};

// ---------------------------------------------------------------------------
// App state — owned by the TUI thread
// ---------------------------------------------------------------------------

struct AppState {
    session: SessionState,
    composer: String,
    /// Tokens accumulated for the currently-streaming assistant response.
    stream_buffer: String,
    is_streaming: bool,
    status: String,
    /// Vertical scroll offset for the transcript pane (lines from bottom).
    scroll: u16,
    /// Whether the right inspector pane is visible.
    show_inspector: bool,
    /// STDB connectivity state for display.
    evidence_status: String,
}

impl AppState {
    fn new(session: SessionState, evidence_available: bool) -> Self {
        Self {
            session,
            composer: String::new(),
            stream_buffer: String::new(),
            is_streaming: false,
            status: "ready".into(),
            scroll: 0,
            show_inspector: false,
            evidence_status: if evidence_available {
                "local-jsonl".into()
            } else {
                "unavailable".into()
            },
        }
    }

    /// Drain all pending events from the controller, updating local state.
    fn apply_events(&mut self, rx: &mut mpsc::UnboundedReceiver<CockpitEvent>) {
        while let Ok(event) = rx.try_recv() {
            match event {
                CockpitEvent::TranscriptAppend(block) => {
                    self.session.transcript.push(block);
                    self.scroll = 0;
                }
                CockpitEvent::RoutingUpdate(routing) => {
                    self.session.routing = routing;
                }
                CockpitEvent::StreamToken(tok) => {
                    self.is_streaming = true;
                    self.stream_buffer.push_str(&tok);
                    self.scroll = 0;
                }
                CockpitEvent::StreamDone {
                    tokens_in,
                    tokens_out,
                    cost,
                } => {
                    self.is_streaming = false;
                    if !self.stream_buffer.is_empty() {
                        let text = std::mem::take(&mut self.stream_buffer);
                        self.session
                            .transcript
                            .push(TranscriptBlock::Assistant(text));
                    }
                    self.status = format!(
                        "done — {} in / {} out | ${:.4}",
                        tokens_in, tokens_out, cost
                    );
                }
                CockpitEvent::StreamError(msg) => {
                    self.is_streaming = false;
                    if !self.stream_buffer.is_empty() {
                        let text = std::mem::take(&mut self.stream_buffer);
                        self.session
                            .transcript
                            .push(TranscriptBlock::Assistant(text));
                    }
                    self.session
                        .transcript
                        .push(TranscriptBlock::Evidence(format!("error: {}", msg)));
                    self.status = "error".into();
                }
                CockpitEvent::StatusUpdate(s) => {
                    self.status = s;
                }
            }
        }
    }

    /// Handle a key event, returning true if the app should quit.
    fn handle_key(
        &mut self,
        code: KeyCode,
        modifiers: KeyModifiers,
        cmd_tx: &mpsc::UnboundedSender<CockpitCommand>,
    ) -> bool {
        match code {
            KeyCode::Esc => {
                let _ = cmd_tx.send(CockpitCommand::Quit);
                return true;
            }
            KeyCode::Enter => {
                if modifiers.contains(KeyModifiers::ALT) {
                    // Alt+Enter: insert newline
                    self.composer.push('\n');
                } else if !self.composer.is_empty() && !self.is_streaming {
                    // Enter: submit
                    let input = std::mem::take(&mut self.composer);
                    self.session
                        .transcript
                        .push(TranscriptBlock::User(input.clone()));
                    self.status = "routing...".into();
                    self.scroll = 0;
                    let _ = cmd_tx.send(CockpitCommand::SubmitInput(input));
                }
            }
            KeyCode::Tab => {
                self.show_inspector = !self.show_inspector;
            }
            KeyCode::Char(c) => {
                if modifiers.contains(KeyModifiers::CONTROL) && c == 'c' {
                    let _ = cmd_tx.send(CockpitCommand::Quit);
                    return true;
                }
                self.composer.push(c);
            }
            KeyCode::Backspace => {
                self.composer.pop();
            }
            KeyCode::Up => {
                self.scroll = self.scroll.saturating_add(1);
            }
            KeyCode::Down => {
                self.scroll = self.scroll.saturating_sub(1);
            }
            _ => {}
        }
        false
    }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

/// Composer height: 1 line of content + 2 for border, growing up to a cap.
const COMPOSER_MIN_HEIGHT: u16 = 3; // 1 content line + 2 border
const COMPOSER_MAX_HEIGHT: u16 = 10; // 8 content lines + 2 border

fn composer_height(text: &str) -> u16 {
    let line_count = if text.is_empty() {
        1
    } else {
        text.lines().count().max(1) + if text.ends_with('\n') { 1 } else { 0 }
    };
    let desired = (line_count as u16) + 2; // +2 for border
    desired.clamp(COMPOSER_MIN_HEIGHT, COMPOSER_MAX_HEIGHT)
}

fn render(f: &mut Frame, state: &AppState) {
    let ch = composer_height(&state.composer);
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Header
            Constraint::Min(6),     // Middle (transcript + optional inspector)
            Constraint::Length(ch), // Composer (dynamic)
            Constraint::Length(1),  // Footer
        ])
        .split(f.area());

    render_header(f, state, outer[0]);
    render_middle(f, state, outer[1]);
    render_composer(f, state, outer[2]);
    render_footer(f, state, outer[3]);
}

fn render_header(f: &mut Frame, state: &AppState, area: Rect) {
    let header_text = format!(
        " {} | {} / {} | {}",
        state.session.session_id,
        state.session.routing.current_provider,
        state.session.routing.current_model,
        state.session.routing.mode,
    );
    let header = Paragraph::new(header_text).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Heiwa Cockpit "),
    );
    f.render_widget(header, area);
}

fn render_middle(f: &mut Frame, state: &AppState, area: Rect) {
    if state.show_inspector {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(70), // Transcript
                Constraint::Percentage(30), // Inspector
            ])
            .split(area);

        render_transcript(f, state, cols[0]);
        render_inspector(f, state, cols[1]);
    } else {
        render_transcript(f, state, area);
    }
}

fn render_transcript(f: &mut Frame, state: &AppState, area: Rect) {
    let mut lines: Vec<Line> = Vec::new();

    for block in &state.session.transcript {
        match block {
            TranscriptBlock::User(text) => {
                lines.push(Line::from(Span::styled(
                    format!("▸ {}", text),
                    Style::default().fg(Color::Cyan),
                )));
                lines.push(Line::from(""));
            }
            TranscriptBlock::Assistant(text) => {
                render_markdown_lines(text, &mut lines, Color::White);
                lines.push(Line::from(""));
            }
            TranscriptBlock::Tool(name, output) => {
                lines.push(Line::from(Span::styled(
                    format!("  [tool: {}]", name),
                    Style::default().fg(Color::Yellow),
                )));
                if !output.is_empty() {
                    for line in output.lines() {
                        lines.push(Line::from(Span::styled(
                            format!("    {}", line),
                            Style::default().fg(Color::Yellow),
                        )));
                    }
                }
            }
            TranscriptBlock::Evidence(text) => {
                let style = Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::ITALIC);
                for (i, line) in text.lines().enumerate() {
                    let prefix = if i == 0 { "  ▪ " } else { "    " };
                    lines.push(Line::from(Span::styled(
                        format!("{}{}", prefix, line),
                        style,
                    )));
                }
            }
        }
    }

    // Append in-progress streaming content with markdown rendering
    if state.is_streaming && !state.stream_buffer.is_empty() {
        render_markdown_lines(&state.stream_buffer, &mut lines, Color::Green);
        lines.push(Line::from(Span::styled(
            "  ▍",
            Style::default().fg(Color::Green),
        )));
    }

    let transcript_height = area.height.saturating_sub(2) as usize;
    let total_lines = lines.len();
    let scroll_offset = if total_lines > transcript_height {
        (total_lines - transcript_height).saturating_sub(state.scroll as usize)
    } else {
        0
    };

    let transcript = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL))
        .wrap(Wrap { trim: false })
        .scroll((scroll_offset as u16, 0));
    f.render_widget(transcript, area);
}

// ---------------------------------------------------------------------------
// Markdown-aware line renderer
// ---------------------------------------------------------------------------

/// Convert markdown text into styled `Line` items for ratatui rendering.
///
/// This is a simple line-by-line state machine — not a full CommonMark parser.
/// It handles the most common patterns in LLM assistant output:
/// - Fenced code blocks (``` ... ```)
/// - Headings (#, ##, ###)
/// - Bullet lists (-, *)
/// - Blockquotes (>)
/// - Inline bold (**text**) and inline code (`code`)
fn render_markdown_lines<'a>(text: &str, lines: &mut Vec<Line<'a>>, base_color: Color) {
    let mut in_code_block = false;
    let mut code_lang = String::new();

    for raw_line in text.lines() {
        let trimmed = raw_line.trim();

        // --- Fenced code block toggle ---
        if trimmed.starts_with("```") {
            if !in_code_block {
                in_code_block = true;
                code_lang = trimmed.trim_start_matches('`').trim().to_string();
                let label = if code_lang.is_empty() {
                    "  ──── code ────".to_string()
                } else {
                    format!("  ──── {} ────", code_lang)
                };
                lines.push(Line::from(Span::styled(
                    label,
                    Style::default().fg(Color::DarkGray),
                )));
            } else {
                in_code_block = false;
                code_lang.clear();
                lines.push(Line::from(Span::styled(
                    "  ────────────────",
                    Style::default().fg(Color::DarkGray),
                )));
            }
            continue;
        }

        // --- Inside code block: render as-is with code styling ---
        if in_code_block {
            lines.push(Line::from(Span::styled(
                format!("  │ {}", raw_line),
                Style::default().fg(Color::Yellow),
            )));
            continue;
        }

        // --- Headings ---
        if trimmed.starts_with("### ") {
            let heading = trimmed.trim_start_matches('#').trim();
            lines.push(Line::from(Span::styled(
                format!("  {}", heading),
                Style::default().fg(base_color).add_modifier(Modifier::BOLD),
            )));
            continue;
        }
        if trimmed.starts_with("## ") {
            let heading = trimmed.trim_start_matches('#').trim();
            lines.push(Line::from(Span::styled(
                format!("  {}", heading),
                Style::default()
                    .fg(base_color)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            )));
            continue;
        }
        if trimmed.starts_with("# ") {
            let heading = trimmed.trim_start_matches('#').trim();
            lines.push(Line::from(Span::styled(
                format!("  {}", heading),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )));
            continue;
        }

        // --- Blockquote ---
        if trimmed.starts_with("> ") || trimmed == ">" {
            let quote_text = trimmed.trim_start_matches('>').trim();
            lines.push(Line::from(Span::styled(
                format!("  │ {}", quote_text),
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::ITALIC),
            )));
            continue;
        }

        // --- Bullet lists ---
        if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
            let item = &trimmed[2..];
            let mut spans = vec![Span::styled(
                "  • ".to_string(),
                Style::default().fg(base_color),
            )];
            parse_inline_spans(item, &mut spans, base_color);
            lines.push(Line::from(spans));
            continue;
        }

        // --- Numbered lists ---
        if trimmed.len() > 2 && trimmed.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            if let Some(rest) = trimmed.split_once(". ").map(|(_, r)| r) {
                let prefix_end = trimmed.find(". ").unwrap_or(0);
                let num = &trimmed[..prefix_end + 1];
                let mut spans = vec![Span::styled(
                    format!("  {} ", num),
                    Style::default().fg(base_color),
                )];
                parse_inline_spans(rest, &mut spans, base_color);
                lines.push(Line::from(spans));
                continue;
            }
        }

        // --- Empty lines ---
        if trimmed.is_empty() {
            lines.push(Line::from(""));
            continue;
        }

        // --- Regular text with inline formatting ---
        let mut spans = vec![Span::raw("  ".to_string())];
        parse_inline_spans(trimmed, &mut spans, base_color);
        lines.push(Line::from(spans));
    }
}

/// Parse inline markdown spans: **bold** and `code`.
fn parse_inline_spans<'a>(text: &str, spans: &mut Vec<Span<'a>>, base_color: Color) {
    let mut remaining = text;
    let bold_style = Style::default().fg(base_color).add_modifier(Modifier::BOLD);
    let code_style = Style::default().fg(Color::Magenta);
    let normal_style = Style::default().fg(base_color);

    while !remaining.is_empty() {
        // Find the nearest inline marker
        let bold_pos = remaining.find("**");
        let code_pos = remaining.find('`');

        match (bold_pos, code_pos) {
            (Some(bp), Some(cp)) if cp < bp => {
                // Inline code comes first
                if cp > 0 {
                    spans.push(Span::styled(remaining[..cp].to_string(), normal_style));
                }
                let after_tick = &remaining[cp + 1..];
                if let Some(end) = after_tick.find('`') {
                    spans.push(Span::styled(after_tick[..end].to_string(), code_style));
                    remaining = &after_tick[end + 1..];
                } else {
                    // Unclosed backtick — render rest as-is
                    spans.push(Span::styled(remaining[cp..].to_string(), normal_style));
                    return;
                }
            }
            (Some(bp), _) => {
                // Bold comes first (or no code)
                if bp > 0 {
                    spans.push(Span::styled(remaining[..bp].to_string(), normal_style));
                }
                let after_stars = &remaining[bp + 2..];
                if let Some(end) = after_stars.find("**") {
                    spans.push(Span::styled(after_stars[..end].to_string(), bold_style));
                    remaining = &after_stars[end + 2..];
                } else {
                    // Unclosed bold — render rest as-is
                    spans.push(Span::styled(remaining[bp..].to_string(), normal_style));
                    return;
                }
            }
            (None, Some(cp)) => {
                // Only inline code
                if cp > 0 {
                    spans.push(Span::styled(remaining[..cp].to_string(), normal_style));
                }
                let after_tick = &remaining[cp + 1..];
                if let Some(end) = after_tick.find('`') {
                    spans.push(Span::styled(after_tick[..end].to_string(), code_style));
                    remaining = &after_tick[end + 1..];
                } else {
                    spans.push(Span::styled(remaining[cp..].to_string(), normal_style));
                    return;
                }
            }
            (None, None) => {
                // No markers — push remainder
                spans.push(Span::styled(remaining.to_string(), normal_style));
                return;
            }
        }
    }
}

fn render_inspector(f: &mut Frame, state: &AppState, area: Rect) {
    let mut lines: Vec<Line> = Vec::new();

    // ── Routing ─────────────────────────────────────────────────────────
    lines.push(Line::from(Span::styled(
        "Routing",
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(Span::styled(
        format!("  provider: {}", state.session.routing.current_provider),
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(Span::styled(
        format!("  model:    {}", state.session.routing.current_model),
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(Span::styled(
        format!("  mode:     {}", state.session.routing.mode),
        Style::default().fg(Color::DarkGray),
    )));
    if let Some(ref explanation) = state.session.routing.explanation {
        lines.push(Line::from(Span::styled(
            format!("  reason:   {}", explanation),
            Style::default().fg(Color::DarkGray),
        )));
    }

    lines.push(Line::from(""));

    // ── Sync ────────────────────────────────────────────────────────────
    lines.push(Line::from(Span::styled(
        "Sync",
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    )));
    let sync_color = if state.evidence_status == "local-jsonl" {
        Color::Green
    } else {
        Color::Yellow
    };
    lines.push(Line::from(Span::styled(
        format!("  evidence: {}", state.evidence_status),
        Style::default().fg(sync_color),
    )));

    lines.push(Line::from(""));

    // ── Latest receipt ──────────────────────────────────────────────────
    lines.push(Line::from(Span::styled(
        "Status",
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(Span::styled(
        format!("  {}", state.status),
        Style::default().fg(Color::DarkGray),
    )));

    let inspector = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(" Inspector "))
        .wrap(Wrap { trim: true });
    f.render_widget(inspector, area);
}

fn render_composer(f: &mut Frame, state: &AppState, area: Rect) {
    let (text, style) = if state.is_streaming {
        (
            " (streaming...)".to_string(),
            Style::default().fg(Color::DarkGray),
        )
    } else {
        // Show cursor indicator at the end of the text
        let display = if state.composer.is_empty() {
            "▍".to_string()
        } else {
            format!("{}▍", state.composer)
        };
        (display, Style::default().fg(Color::White))
    };
    let composer = Paragraph::new(text)
        .block(Block::default().borders(Borders::ALL).title(" > "))
        .style(style)
        .wrap(Wrap { trim: false });
    f.render_widget(composer, area);
}

fn render_footer(f: &mut Frame, state: &AppState, area: Rect) {
    let inspector_hint = if state.show_inspector {
        "Tab hide inspector"
    } else {
        "Tab inspector"
    };
    let footer = Paragraph::new(Line::from(vec![
        Span::styled(
            format!(" [{}] ", state.status),
            Style::default().fg(Color::DarkGray),
        ),
        Span::raw(format!(
            "Esc quit · ↑↓ scroll · Alt+Enter newline · {}",
            inspector_hint
        )),
    ]));
    f.render_widget(footer, area);
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Run the interactive cockpit TUI. Blocks the calling thread until the user
/// quits. The controller task communicates via the provided channels.
pub fn run_cockpit(
    mut event_rx: mpsc::UnboundedReceiver<CockpitEvent>,
    cmd_tx: mpsc::UnboundedSender<CockpitCommand>,
    initial_session: SessionState,
    evidence_available: bool,
) -> Result<()> {
    terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut state = AppState::new(initial_session, evidence_available);

    loop {
        // 1. Drain controller events
        state.apply_events(&mut event_rx);

        // 2. Render
        terminal.draw(|f| render(f, &state))?;

        // 3. Poll terminal input (50ms timeout keeps the UI responsive)
        if event::poll(std::time::Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if state.handle_key(key.code, key.modifiers, &cmd_tx) {
                    break;
                }
            }
        }
    }

    // Cleanup
    terminal::disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}

// Re-export the old function for backwards compat with any tests
pub fn render_cockpit(f: &mut Frame, state: &SessionState) {
    let app = AppState::new(state.clone(), false);
    render(f, &app);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composer_height_empty() {
        assert_eq!(composer_height(""), COMPOSER_MIN_HEIGHT);
    }

    #[test]
    fn composer_height_single_line() {
        assert_eq!(composer_height("hello"), COMPOSER_MIN_HEIGHT);
    }

    #[test]
    fn composer_height_two_lines() {
        assert_eq!(composer_height("line1\nline2"), 4); // 2 lines + 2 border
    }

    #[test]
    fn composer_height_trailing_newline() {
        // "line1\n" means cursor is on line 2 (empty), so 2 lines of content
        assert_eq!(composer_height("line1\n"), 4);
    }

    #[test]
    fn composer_height_capped() {
        let many_lines = "a\n".repeat(20);
        assert_eq!(composer_height(&many_lines), COMPOSER_MAX_HEIGHT);
    }

    // --- Markdown renderer tests ---

    fn render_to_lines(text: &str) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        render_markdown_lines(text, &mut lines, Color::White);
        lines
    }

    #[test]
    fn markdown_heading_h1() {
        let lines = render_to_lines("# Hello World");
        assert_eq!(lines.len(), 1);
        // H1 should be Cyan + Bold
        let span = &lines[0].spans[0];
        assert!(span.content.contains("Hello World"));
        assert_eq!(span.style.fg, Some(Color::Cyan));
    }

    #[test]
    fn markdown_heading_h2() {
        let lines = render_to_lines("## Subheading");
        assert_eq!(lines.len(), 1);
        let span = &lines[0].spans[0];
        assert!(span.content.contains("Subheading"));
        assert!(span.style.add_modifier.contains(Modifier::UNDERLINED));
    }

    #[test]
    fn markdown_fenced_code_block() {
        let text = "before\n```rust\nlet x = 1;\n```\nafter";
        let lines = render_to_lines(text);
        // before, code-label, code-line, code-end, after = 5 lines
        assert_eq!(lines.len(), 5);
        // Code label should mention "rust"
        assert!(lines[1].spans[0].content.contains("rust"));
        // Code line should have │ prefix
        assert!(lines[2].spans[0].content.contains("│"));
        assert!(lines[2].spans[0].content.contains("let x = 1;"));
        // Code end should be a separator
        assert!(lines[3].spans[0].content.contains("────"));
    }

    #[test]
    fn markdown_bullet_list() {
        let lines = render_to_lines("- first item\n- second item");
        assert_eq!(lines.len(), 2);
        // Should have bullet prefix
        assert!(lines[0].spans[0].content.contains("•"));
    }

    #[test]
    fn markdown_blockquote() {
        let lines = render_to_lines("> quoted text");
        assert_eq!(lines.len(), 1);
        assert!(lines[0].spans[0].content.contains("│"));
        assert!(lines[0].spans[0].content.contains("quoted text"));
    }

    #[test]
    fn markdown_inline_code() {
        let lines = render_to_lines("Use `cargo test` to run");
        assert_eq!(lines.len(), 1);
        // Should have multiple spans: prefix, "Use ", "cargo test", " to run"
        assert!(lines[0].spans.len() >= 3);
        // Find the code span (Magenta)
        let code_span = lines[0]
            .spans
            .iter()
            .find(|s| s.style.fg == Some(Color::Magenta));
        assert!(code_span.is_some());
        assert_eq!(code_span.unwrap().content.as_ref(), "cargo test");
    }

    #[test]
    fn markdown_inline_bold() {
        let lines = render_to_lines("This is **important** text");
        assert_eq!(lines.len(), 1);
        let bold_span = lines[0]
            .spans
            .iter()
            .find(|s| s.style.add_modifier.contains(Modifier::BOLD));
        assert!(bold_span.is_some());
        assert_eq!(bold_span.unwrap().content.as_ref(), "important");
    }

    #[test]
    fn markdown_mixed_content() {
        let text = "# Title\n\nSome text with `code` and **bold**.\n\n```python\nprint('hello')\n```\n\n- item one\n- item two";
        let lines = render_to_lines(text);
        // Title + empty + text + empty + code-label + code-line + code-end + empty + item1 + item2
        assert_eq!(lines.len(), 10);
    }

    #[test]
    fn markdown_empty_text() {
        let lines = render_to_lines("");
        // Empty string produces no lines (no newlines to iterate)
        assert!(lines.is_empty());
    }

    #[test]
    fn markdown_plain_text_no_formatting() {
        let lines = render_to_lines("Just plain text here");
        assert_eq!(lines.len(), 1);
        // Should have 2 spans: "  " prefix + text
        assert_eq!(lines[0].spans.len(), 2);
    }
}
