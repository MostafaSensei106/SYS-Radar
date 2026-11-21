use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Rect},
    style::{Modifier, Style},
    widgets::{Cell, Clear, Paragraph, Row, Table},
};

use crate::{
    domain::telemetry::SystemSnapshot,
    ui::{
        state::{UiState, filtered_processes, format_bytes},
        theme,
    },
};

pub fn render_processes(
    frame: &mut Frame,
    area: Rect,
    snapshot: &SystemSnapshot,
    state: &mut UiState,
) {
    let processes = filtered_processes(snapshot, &state.process_filter);
    let rows = processes.iter().map(|process| {
        let memory_percent =
            process.rss_bytes as f64 / snapshot.memory.total_bytes.max(1) as f64 * 100.0;
        Row::new(vec![
            Cell::from(process.pid.to_string()).style(Style::default().fg(theme::MUTED)),
            Cell::from(process.name.clone()),
            Cell::from(process.threads.to_string()),
            Cell::from(format!("{:.1}%", process.cpu_pct)),
            Cell::from(format!("{memory_percent:.1}%")),
            Cell::from(format_bytes(process.rss_bytes)),
            Cell::from(
                process
                    .gpu_util_pct
                    .map_or_else(|| "—".into(), |value| format!("{value}%")),
            ),
            Cell::from(
                process
                    .gpu_mem_bytes
                    .map_or_else(|| "—".into(), format_bytes),
            ),
        ])
    });
    let count = format!("{} / {}", processes.len(), snapshot.processes.len());
    let title = format!(" Processes · sort {} · {count} ", state.sort_by.label());
    let filter = if state.filter_active {
        Paragraph::new(format!(" Filter  /{}▏", state.process_filter))
            .style(Style::default().fg(theme::AMBER))
    } else if state.process_filter.is_empty() {
        Paragraph::new("/ filter · GPU SM and VRAM are shown when NVML reports per-process data")
            .style(Style::default().fg(theme::MUTED))
    } else {
        Paragraph::new(format!(" Filter  {}  ·  x clears", state.process_filter))
            .style(Style::default().fg(theme::BLUE))
    };
    let header = Row::new([
        "PID", "Process", "Threads", "CPU", "MEM", "RSS", "GPU SM", "GPU VRAM",
    ])
    .style(
        Style::default()
            .fg(theme::CYAN)
            .add_modifier(Modifier::BOLD),
    );
    let table = Table::new(
        rows,
        [
            Constraint::Length(8),
            Constraint::Min(20),
            Constraint::Length(8),
            Constraint::Length(9),
            Constraint::Length(8),
            Constraint::Length(12),
            Constraint::Length(8),
            Constraint::Length(10),
        ],
    )
    .header(header)
    .column_spacing(1)
    .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD))
    .highlight_symbol("▸ ")
    .block(theme::panel(title));
    let chunks = ratatui::layout::Layout::default()
        .direction(ratatui::layout::Direction::Vertical)
        .constraints([Constraint::Min(2), Constraint::Length(1)])
        .split(area);
    frame.render_stateful_widget(table, chunks[0], &mut state.table_state);
    frame.render_widget(filter, chunks[1]);
}

pub fn render_kill_confirmation(frame: &mut Frame, snapshot: &SystemSnapshot, pid: u32) {
    let area = frame.area();
    let width = 56.min(area.width.saturating_sub(4));
    let height = 7.min(area.height.saturating_sub(2));
    let popup = Rect::new(
        area.x + (area.width.saturating_sub(width)) / 2,
        area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    );
    let name = snapshot
        .processes
        .iter()
        .find(|process| process.pid == pid)
        .map(|process| process.name.as_str())
        .unwrap_or("process");
    frame.render_widget(Clear, popup);
    let message =
        format!("Terminate {name} (PID {pid})?\nThis sends the system termination signal.");
    frame.render_widget(
        Paragraph::new(message)
            .alignment(Alignment::Center)
            .style(Style::default().fg(theme::TEXT).bg(theme::SURFACE))
            .block(theme::panel(" Confirm process termination ")),
        popup,
    );
}
