use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Cell, Paragraph, Row, Table},
};

use crate::{
    domain::telemetry::SystemSnapshot,
    ui::{
        state::{UiState, format_bytes},
        theme,
    },
};

pub fn render_system(frame: &mut Frame, area: Rect, snapshot: &SystemSnapshot, state: &UiState) {
    let panels = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(area);
    render_cores(frame, panels[0], snapshot);
    render_disks(frame, panels[1], snapshot, state);
}

fn render_cores(frame: &mut Frame, area: Rect, snapshot: &SystemSnapshot) {
    let lines = snapshot
        .cores
        .chunks(3)
        .map(|chunk| {
            Line::from(
                chunk
                    .iter()
                    .map(|core| {
                        let color = if core.usage_pct > 85.0 {
                            theme::RED
                        } else if core.usage_pct > 55.0 {
                            theme::AMBER
                        } else {
                            theme::GREEN
                        };
                        Span::styled(
                            format!(" C{:02} {:>3.0}% ", core.id, core.usage_pct),
                            Style::default().fg(color),
                        )
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        Paragraph::new(lines).block(theme::panel(" Per-core activity ")),
        area,
    );
}

fn render_disks(frame: &mut Frame, area: Rect, snapshot: &SystemSnapshot, state: &UiState) {
    let rows = snapshot.disks.iter().map(|disk| {
        let used = disk.total_bytes.saturating_sub(disk.free_bytes);
        let percent = used
            .saturating_mul(100)
            .checked_div(disk.total_bytes)
            .unwrap_or(0);
        let usage_color = if percent >= 90 {
            theme::RED
        } else if percent >= 75 {
            theme::AMBER
        } else {
            theme::GREEN
        };
        let filled = (percent.min(100) * 6 / 100) as usize;
        let used_cell = Line::from(vec![
            Span::styled("█".repeat(filled), Style::default().fg(usage_color)),
            Span::styled("░".repeat(6 - filled), Style::default().fg(theme::MUTED)),
            Span::raw(format!(" {percent:>3}%")),
        ]);
        let trend = state
            .storage_history
            .get(&disk.mount)
            .map(storage_trend)
            .unwrap_or_else(|| "············".into());
        Row::new(vec![
            Cell::from(disk.mount.clone()),
            Cell::from(used_cell),
            Cell::from(format!(
                "R {} W {}",
                format_bytes(disk.read_bytes_per_sec),
                format_bytes(disk.write_bytes_per_sec)
            )),
            Cell::from(Span::styled(trend, Style::default().fg(theme::CYAN))),
        ])
    });
    let table = Table::new(
        rows,
        [
            Constraint::Length(12),
            Constraint::Length(12),
            Constraint::Min(16),
            Constraint::Length(12),
        ],
    )
    .header(
        Row::new(["Mount", "Used", "Read / write", "I/O trend"]).style(
            Style::default()
                .fg(theme::CYAN)
                .add_modifier(Modifier::BOLD),
        ),
    )
    .column_spacing(1)
    .block(theme::panel(" Storage "));
    frame.render_widget(table, area);
}

fn storage_trend(history: &crate::domain::telemetry::RingHistory<30>) -> String {
    const LEVELS: [&str; 8] = ["▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"];
    let values = history.to_vec();
    let peak = values.iter().copied().max().unwrap_or(0).max(1);
    values
        .iter()
        .rev()
        .take(12)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|value| LEVELS[(value.saturating_mul(7) / peak).min(7) as usize])
        .collect()
}
