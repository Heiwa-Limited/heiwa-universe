use ratatui::{
    layout::{Constraint, Direction, Layout},
    widgets::{Block, Borders, Paragraph},
    Frame,
};
use heiwa_protocol::SessionState;

pub fn render_cockpit(f: &mut Frame, state: &SessionState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(10),   // Transcript
            Constraint::Length(3), // Composer
            Constraint::Length(1), // Footer
        ])
        .split(f.size());

    // Header
    let header = Paragraph::new(format!("Session: {} | Provider: {}", state.session_id, state.routing.current_provider))
        .block(Block::default().borders(Borders::ALL).title(" Heiwa Cockpit "));
    f.render_widget(header, chunks[0]);

    // Transcript (Placeholder)
    let transcript = Paragraph::new("Transcript will stream here...")
        .block(Block::default().borders(Borders::LEFT | Borders::RIGHT));
    f.render_widget(transcript, chunks[1]);

    // Composer
    let composer = Paragraph::new("> ")
        .block(Block::default().borders(Borders::ALL).title(" Command "));
    f.render_widget(composer, chunks[2]);

    // Footer
    let footer = Paragraph::new(format!("Mode: {} | Cost: $0.00", state.routing.mode));
    f.render_widget(footer, chunks[3]);
}
