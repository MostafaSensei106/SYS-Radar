use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Gauge, Paragraph, Sparkline},
};

use crate::{
    domain::telemetry::SystemSnapshot,
    ui::{
        state::{UiState, format_bytes},
        theme,
    },
};

pub fn render_metrics(frame: &mut Frame, area: Rect, snapshot: &SystemSnapshot, state: &UiState) {
    let panels = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(34),
            Constraint::Percentage(33),
            Constraint::Percentage(33),
        ])
        .split(area);
    render_cpu(frame, panels[0], snapshot, state);
    render_memory(frame, panels[1], snapshot);
    render_gpu(frame, panels[2], snapshot, state);
}

fn render_cpu(frame: &mut Frame, area: Rect, snapshot: &SystemSnapshot, state: &UiState) {
    let content = theme::panel(" CPU ");
    let inner = content.inner(area);
    frame.render_widget(content, area);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .split(inner);

    let usage = snapshot.global_cpu_pct.clamp(0.0, 100.0);
    frame.render_widget(
        Gauge::default()
            .gauge_style(Style::default().fg(if usage > 85.0 {
                theme::RED
            } else {
                theme::CYAN
            }))
            .percent(usage as u16)
            .label(format!("Total load  {usage:.1}%")),
        rows[0],
    );
    let mean_frequency = if snapshot.cores.is_empty() {
        0
    } else {
        snapshot.cores.iter().map(|core| core.freq_mhz).sum::<u64>() / snapshot.cores.len() as u64
    };
    frame.render_widget(
        Paragraph::new(format!(
            "{} cores · {} threads · avg {mean_frequency} MHz",
            snapshot.specs.physical_cores, snapshot.specs.logical_cores
        ))
        .style(Style::default().fg(theme::MUTED)),
        rows[1],
    );
    let history = state.cpu_history.to_vec();
    frame.render_widget(
        Sparkline::default()
            .data(&history)
            .max(100)
            .style(Style::default().fg(theme::CYAN)),
        rows[2],
    );
}

fn render_memory(frame: &mut Frame, area: Rect, snapshot: &SystemSnapshot) {
    let memory = &snapshot.memory;
    let content = theme::panel(" Memory ");
    let inner = content.inner(area);
    frame.render_widget(content, area);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);

    let percent =
        ((memory.used_bytes as f64 / memory.total_bytes.max(1) as f64) * 100.0).clamp(0.0, 100.0);
    frame.render_widget(
        Gauge::default()
            .gauge_style(Style::default().fg(theme::PURPLE))
            .percent(percent as u16)
            .label(format!(
                "Used  {} / {} · {:.0}%",
                format_bytes(memory.used_bytes),
                format_bytes(memory.total_bytes),
                percent
            )),
        rows[0],
    );
    frame.render_widget(
        Paragraph::new(format!("Cache  {}", format_bytes(memory.cached_bytes)))
            .style(Style::default().fg(theme::MUTED)),
        rows[1],
    );
    frame.render_widget(
        Paragraph::new(format!(
            "Swap  {} / {}",
            format_bytes(memory.swap_used_bytes),
            format_bytes(memory.swap_total_bytes)
        ))
        .style(Style::default().fg(theme::MUTED)),
        rows[2],
    );
}

fn render_gpu(frame: &mut Frame, area: Rect, snapshot: &SystemSnapshot, state: &UiState) {
    let Some(gpu) = &snapshot.gpu else {
        frame.render_widget(
            Paragraph::new("GPU telemetry unavailable (NVML not detected)")
                .style(Style::default().fg(theme::MUTED))
                .block(theme::panel(" GPU ")),
            area,
        );
        return;
    };

    let title = format!(
        " {} · {}°C · {}W · {}MHz ",
        gpu.name,
        gpu.temp_c,
        gpu.power_mw / 1000,
        gpu.clock_graphics_mhz
    );
    let content = theme::panel(title);
    let inner = content.inner(area);
    frame.render_widget(content, area);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .split(inner);

    frame.render_widget(
        Gauge::default()
            .gauge_style(Style::default().fg(theme::GREEN))
            .percent(gpu.util_pct.min(100) as u16)
            .label(format!("Core load  {}%", gpu.util_pct)),
        rows[0],
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("VRAM  ", Style::default().fg(theme::MUTED)),
            Span::raw(format!(
                "{} / {}",
                format_bytes(gpu.mem_used),
                format_bytes(gpu.mem_total)
            )),
        ])),
        rows[1],
    );
    let history = state.gpu_history.to_vec();
    frame.render_widget(
        Sparkline::default()
            .data(&history)
            .max(100)
            .style(Style::default().fg(theme::GREEN)),
        rows[2],
    );
}
