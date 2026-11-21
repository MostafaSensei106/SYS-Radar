use crossterm::terminal::size;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Gauge, Paragraph, Row, Sparkline, Table, TableState},
};

use crate::domain::telemetry::{RingHistory, SystemSnapshot};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SortField {
    Cpu,
    Memory,
    Threads,
}

pub struct UiState {
    pub sort_by: SortField,
    pub table_state: TableState,
    pub status_banner: String,
    pub cpu_history: RingHistory<60>,
    pub gpu_history: RingHistory<60>,
    pub process_filter: String,
    pub filter_active: bool,
}

impl Default for UiState {
    fn default() -> Self {
        let mut table_state = TableState::default();
        table_state.select(Some(0));
        Self {
            sort_by: SortField::Cpu,
            table_state,
            status_banner: String::new(),
            cpu_history: RingHistory::default(),
            gpu_history: RingHistory::default(),
            process_filter: String::new(),
            filter_active: false,
        }
    }
}

pub fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.2} GiB", b / GB)
    } else if b >= MB {
        format!("{:.1} MiB", b / MB)
    } else if b >= KB {
        format!("{:.0} KiB", b / KB)
    } else {
        format!("{} B", bytes)
    }
}

pub fn filtered_processes<'a>(
    snap: &'a SystemSnapshot,
    filter: &str,
) -> Vec<&'a crate::domain::telemetry::ProcessTelemetry> {
    let needle = filter.to_lowercase();
    snap.processes
        .iter()
        .filter(|process| {
            needle.is_empty()
                || process.name.to_lowercase().contains(&needle)
                || process.pid.to_string().contains(filter)
        })
        .collect()
}

pub fn render_dashboard(f: &mut Frame, snap: &SystemSnapshot, ui: &mut UiState) {
    let root = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(4),
            Constraint::Length(6),
            Constraint::Length(6),
            Constraint::Min(8),
        ])
        .split(f.area());

    let (cols, rows) = size().unwrap_or((0, 0));
    let up_h = snap.uptime_secs / 3600;
    let up_m = (snap.uptime_secs % 3600) / 60;

    let header = Paragraph::new(vec![
        Line::from(vec![
            Span::styled(
                format!(" {} ", snap.specs.hostname),
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(format!(
                " {} ({}) | Kernel {} | Uptime {}h {}m | ",
                snap.specs.os_name, snap.specs.arch, snap.specs.kernel_version, up_h, up_m
            )),
            Span::styled(
                format!(
                    "↓ {}/s  ↑ {}/s",
                    format_bytes(snap.net.rx_bytes_per_sec),
                    format_bytes(snap.net.tx_bytes_per_sec)
                ),
                Style::default().fg(Color::Green),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                format!(
                    "CPU: {} ({}P/{}T) | DE: {} | Term: {} ({}x{}) | Shell: {}",
                    snap.specs.cpu_model,
                    snap.specs.physical_cores,
                    snap.specs.logical_cores,
                    snap.specs.desktop_env,
                    snap.specs.terminal_name,
                    cols,
                    rows,
                    snap.specs.shell_name
                ),
                Style::default().fg(Color::Gray),
            ),
            Span::raw("  / filter  x clear  [c]CPU [m]RAM [t]Thr [k]Kill [q]Quit "),
            Span::styled(&ui.status_banner, Style::default().fg(Color::Yellow)),
        ]),
    ])
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Hardware & Session Telemetry "),
    );
    f.render_widget(header, root[0]);

    let row2 = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(34),
            Constraint::Percentage(33),
            Constraint::Percentage(33),
        ])
        .split(root[1]);

    let cpu_split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Length(3)])
        .split(row2[0]);

    let cpu_gauge = Gauge::default()
        .block(Block::default().borders(Borders::ALL).title(" CPU Total "))
        .gauge_style(Style::default().fg(if snap.global_cpu_pct > 80.0 {
            Color::Red
        } else {
            Color::Cyan
        }))
        .percent(snap.global_cpu_pct.clamp(0.0, 100.0) as u16)
        .label(format!("{:.1}%", snap.global_cpu_pct));
    f.render_widget(cpu_gauge, cpu_split[0]);

    let cpu_hist = ui.cpu_history.to_vec();
    let cpu_spark = Sparkline::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" CPU History "),
        )
        .data(&cpu_hist)
        .max(100)
        .style(Style::default().fg(Color::Cyan));
    f.render_widget(cpu_spark, cpu_split[1]);

    let mem_pct = ((snap.memory.used_bytes as f64 / snap.memory.total_bytes.max(1) as f64) * 100.0)
        .clamp(0.0, 100.0) as u16;
    let mem_gauge = Gauge::default()
        .block(Block::default().borders(Borders::ALL).title(format!(
            " RAM (Cache: {} | Swap: {}/{}) ",
            format_bytes(snap.memory.cached_bytes),
            format_bytes(snap.memory.swap_used_bytes),
            format_bytes(snap.memory.swap_total_bytes)
        )))
        .gauge_style(Style::default().fg(Color::Magenta))
        .percent(mem_pct)
        .label(format!(
            "{} / {} ({}%)",
            format_bytes(snap.memory.used_bytes),
            format_bytes(snap.memory.total_bytes),
            mem_pct
        ));
    f.render_widget(mem_gauge, row2[1]);

    if let Some(gpu) = &snap.gpu {
        let gpu_split = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Length(3)])
            .split(row2[2]);

        let gpu_gauge = Gauge::default()
            .block(Block::default().borders(Borders::ALL).title(format!(
                " {} ({}°C | {}W | {}MHz) ",
                gpu.name,
                gpu.temp_c,
                gpu.power_mw / 1000,
                gpu.clock_graphics_mhz
            )))
            .gauge_style(Style::default().fg(Color::Green))
            .percent(gpu.util_pct.min(100) as u16)
            .label(format!(
                "Core {}% | VRAM {}/{}",
                gpu.util_pct,
                format_bytes(gpu.mem_used),
                format_bytes(gpu.mem_total)
            ));
        f.render_widget(gpu_gauge, gpu_split[0]);

        let gpu_hist = ui.gpu_history.to_vec();
        let gpu_spark = Sparkline::default()
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" GPU History "),
            )
            .data(&gpu_hist)
            .max(100)
            .style(Style::default().fg(Color::Green));
        f.render_widget(gpu_spark, gpu_split[1]);
    } else {
        let no_gpu = Paragraph::new("NVML inactive")
            .block(Block::default().borders(Borders::ALL).title(" GPU "));
        f.render_widget(no_gpu, row2[2]);
    }

    let row3 = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(root[2]);

    let core_lines: Vec<Line> = snap
        .cores
        .chunks(4)
        .map(|chunk| {
            let spans: Vec<Span> = chunk
                .iter()
                .map(|c| {
                    let color = if c.usage_pct > 75.0 {
                        Color::Red
                    } else if c.usage_pct > 35.0 {
                        Color::Yellow
                    } else {
                        Color::Green
                    };
                    Span::styled(
                        format!("C{:02}:{:4.0}% ({}MHz)  ", c.id, c.usage_pct, c.freq_mhz),
                        Style::default().fg(color),
                    )
                })
                .collect();
            Line::from(spans)
        })
        .collect();
    let cores_widget = Paragraph::new(core_lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Per-Core Load & Clock "),
    );
    f.render_widget(cores_widget, row3[0]);

    let disk_rows = snap.disks.iter().map(|d| {
        let used = d.total_bytes.saturating_sub(d.free_bytes);
        let pct = used
            .saturating_mul(100)
            .checked_div(d.total_bytes)
            .unwrap_or(0);
        Row::new(vec![
            Cell::from(d.mount.clone()),
            Cell::from(d.device.clone()),
            Cell::from(d.fs_type.clone()),
            Cell::from(format!(
                "{}/{} ({}%)",
                format_bytes(used),
                format_bytes(d.total_bytes),
                pct
            )),
            Cell::from(format!(
                "R:{}/s W:{}/s",
                format_bytes(d.read_bytes_per_sec),
                format_bytes(d.write_bytes_per_sec)
            )),
        ])
    });
    let disk_table = Table::new(
        disk_rows,
        [
            Constraint::Length(12),
            Constraint::Length(16),
            Constraint::Length(8),
            Constraint::Length(24),
            Constraint::Min(22),
        ],
    )
    .header(
        Row::new(vec!["Mount", "Device", "FS", "Used / Total", "Live I/O"]).style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
    )
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Storage & Live Block I/O "),
    );
    f.render_widget(disk_table, row3[1]);

    let visible_processes = filtered_processes(snap, &ui.process_filter);
    let proc_rows = visible_processes.iter().map(|p| {
        Row::new(vec![
            Cell::from(p.pid.to_string()),
            Cell::from(p.state.to_string()),
            Cell::from(p.name.clone()),
            Cell::from(p.threads.to_string()),
            Cell::from(format!("{:.1}%", p.cpu_pct)),
            Cell::from(format_bytes(p.rss_bytes)),
        ])
    });

    let sort_tag = match ui.sort_by {
        SortField::Cpu => "CPU",
        SortField::Memory => "RSS",
        SortField::Threads => "Threads",
    };
    let proc_table = Table::new(
        proc_rows,
        [
            Constraint::Length(9),
            Constraint::Length(5),
            Constraint::Min(24),
            Constraint::Length(9),
            Constraint::Length(10),
            Constraint::Length(14),
        ],
    )
    .header(
        Row::new(vec!["PID", "St", "Process", "Thr", "CPU %", "RSS Memory"]).style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
    )
    .row_highlight_style(
        Style::default()
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    )
    .block(Block::default().borders(Borders::ALL).title(format!(
        " Processes · {} · filter: {}{} ",
        sort_tag,
        if ui.process_filter.is_empty() {
            "all"
        } else {
            &ui.process_filter
        },
        if ui.filter_active { "▏" } else { "" }
    )));
    f.render_stateful_widget(proc_table, root[3], &mut ui.table_state);
}
