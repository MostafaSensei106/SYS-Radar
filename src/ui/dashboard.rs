use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
};

use crate::domain::telemetry::SystemSnapshot;

use super::{state::UiState, theme, widgets};

pub fn render_dashboard(frame: &mut Frame, snapshot: &SystemSnapshot, state: &mut UiState) {
    let area = frame.area();
    frame.render_widget(
        ratatui::widgets::Block::default().style(theme::body()),
        area,
    );
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(4),
            Constraint::Length(7),
            Constraint::Length(7),
            Constraint::Min(6),
            Constraint::Length(1),
        ])
        .split(area);

    widgets::render_header(frame, sections[0], snapshot, &state.status_banner);
    widgets::render_metrics(frame, sections[1], snapshot, state);
    widgets::render_system(frame, sections[2], snapshot, state);
    widgets::render_processes(frame, sections[3], snapshot, state);
    widgets::render_footer(
        frame,
        sections[4],
        state.filter_active,
        state.pending_kill.is_some(),
    );
    if let Some(pid) = state.pending_kill {
        widgets::render_kill_confirmation(frame, snapshot, pid);
    }
}
