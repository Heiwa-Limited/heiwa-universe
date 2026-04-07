use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
    execute,
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

use heiwa_protocol::{
    CockpitCommand, CockpitEvent, SessionState, TranscriptBlock,
};

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
    stdb_status: String,
}

impl AppState {
    fn new(session: SessionState, stdb_connected: bool) -> Self {
        Self {
            session,
            composer: String::new(),
            stream_buffer: String::new(),
            is_streaming: false,
            status: "ready".into(),
            scroll: 0,
            show_inspector: false,
            stdb_status: if stdb_connected {
                "connected".into()
            } else {
                "offline".into()
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
                if !self.composer.is_empty() && !self.is_streaming {
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

fn render(f: &mut Frame, state: &AppState) {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Header
            Constraint::Min(6),    // Middle (transcript + optional inspector)
            Constraint::Length(3), // Composer
            Constraint::Length(1), // Footer
        ])
        .split(f.size());

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
    let header = Paragraph::new(header_text)
        .block(Block::default().borders(Borders::ALL).title(" Heiwa Cockpit "));
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
                for line in text.lines() {
                    lines.push(Line::from(Span::styled(
                        format!("  {}", line),
                        Style::default().fg(Color::White),
                    )));
                }
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

    // Append in-progress streaming content
    if state.is_streaming && !state.stream_buffer.is_empty() {
        for line in state.stream_buffer.lines() {
            lines.push(Line::from(Span::styled(
                format!("  {}", line),
                Style::default().fg(Color::Green),
            )));
        }
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
    let sync_color = if state.stdb_status == "connected" {
        Color::Green
    } else {
        Color::Yellow
    };
    lines.push(Line::from(Span::styled(
        format!("  stdb: {}", state.stdb_status),
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
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Inspector "),
        )
        .wrap(Wrap { trim: true });
    f.render_widget(inspector, area);
}

fn render_composer(f: &mut Frame, state: &AppState, area: Rect) {
    let composer_display = if state.is_streaming {
        " (streaming...)".to_string()
    } else {
        state.composer.clone()
    };
    let composer = Paragraph::new(composer_display)
        .block(Block::default().borders(Borders::ALL).title(" > "))
        .style(if state.is_streaming {
            Style::default().fg(Color::DarkGray)
        } else {
            Style::default().fg(Color::White)
        });
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
        Span::raw(format!("Esc quit · ↑↓ scroll · {}", inspector_hint)),
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
    stdb_connected: bool,
) -> Result<()> {
    terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut state = AppState::new(initial_session, stdb_connected);

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
