use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Cell, Paragraph, Row, Table},
};

use crate::{
    domain::telemetry::SystemSnapshot,
    ui::{state::format_bytes, theme},
};

pub fn render_system(frame: &mut Frame, area: Rect, snapshot: &SystemSnapshot) {
    let panels = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
        .split(area);
    render_cores(frame, panels[0], snapshot);
    render_disks(frame, panels[1], snapshot);
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

fn render_disks(frame: &mut Frame, area: Rect, snapshot: &SystemSnapshot) {
    let rows = snapshot.disks.iter().map(|disk| {
        let used = disk.total_bytes.saturating_sub(disk.free_bytes);
        let percent = used
            .saturating_mul(100)
            .checked_div(disk.total_bytes)
            .unwrap_or(0);
        Row::new(vec![
            Cell::from(disk.mount.clone()),
            Cell::from(disk.device.clone()),
            Cell::from(disk.fs_type.clone()),
            Cell::from(format!(
                "{} / {} · {percent}%",
                format_bytes(used),
                format_bytes(disk.total_bytes)
            )),
            Cell::from(format!(
                "R {}  W {}",
                format_bytes(disk.read_bytes_per_sec),
                format_bytes(disk.write_bytes_per_sec)
            )),
        ])
    });
    let table = Table::new(
        rows,
        [
            Constraint::Length(12),
            Constraint::Length(14),
            Constraint::Length(7),
            Constraint::Min(23),
            Constraint::Min(20),
        ],
    )
    .header(
        Row::new(["Mount", "Device", "FS", "Used / total", "I/O per sec"]).style(
            Style::default()
                .fg(theme::CYAN)
                .add_modifier(Modifier::BOLD),
        ),
    )
    .column_spacing(1)
    .block(theme::panel(" Storage "));
    frame.render_widget(table, area);
}
