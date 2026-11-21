use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::{domain::telemetry::SystemSnapshot, ui::theme};

pub fn render_header(frame: &mut Frame, area: Rect, snapshot: &SystemSnapshot, status: &str) {
    let uptime = format!(
        "{}h {:02}m",
        snapshot.uptime_secs / 3600,
        (snapshot.uptime_secs % 3600) / 60
    );
    let line = Line::from(vec![
        Span::styled(
            " SYS RADAR ",
            Style::default()
                .fg(theme::BG)
                .bg(theme::CYAN)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("  {}  ", snapshot.specs.hostname),
            Style::default()
                .fg(theme::TEXT)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(
                "{} · {} · up {}",
                snapshot.specs.os_name, snapshot.specs.arch, uptime
            ),
            Style::default().fg(theme::MUTED),
        ),
        Span::raw("   "),
        Span::styled(
            format!(
                "↓ {}  ↑ {}",
                super::super::state::format_bytes(snapshot.net.rx_bytes_per_sec),
                super::super::state::format_bytes(snapshot.net.tx_bytes_per_sec)
            ),
            Style::default().fg(theme::GREEN),
        ),
    ]);
    let details = if status.is_empty() {
        Line::from(vec![
            Span::styled(
                "CPU ",
                Style::default()
                    .fg(theme::BLUE)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{}  ", snapshot.specs.cpu_model),
                Style::default().fg(theme::TEXT),
            ),
            Span::styled(
                format!(
                    "{} cores / {} threads",
                    snapshot.specs.physical_cores, snapshot.specs.logical_cores
                ),
                Style::default().fg(theme::MUTED),
            ),
        ])
    } else {
        Line::from(Span::styled(
            format!("● {status}"),
            Style::default().fg(theme::AMBER),
        ))
    };
    frame.render_widget(
        Paragraph::new(vec![line, details]).block(theme::panel(" Host overview ")),
        area,
    );
}
