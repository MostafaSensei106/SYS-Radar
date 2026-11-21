// Path: src/main.rs
use std::{cmp::Reverse, io, thread, time::Duration};

use anyhow::{Context, Result};
use crossbeam_channel::{bounded, select, tick};
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

use sys_radar::{
    domain::telemetry::SystemSnapshot,
    hal::create_collector,
    ui::{SortField, UiState, dashboard::render_dashboard, filtered_processes},
};

enum ControlCmd {
    Kill(u32),
    Shutdown,
}

fn restore_terminal() {
    let _ = disable_raw_mode();
    let _ = execute!(io::stdout(), LeaveAlternateScreen);
}

fn main() -> Result<()> {
    let mut collector = create_collector().context("Failed to initialize Hardware Collector")?;
    thread::sleep(Duration::from_millis(120));
    let initial_snap = collector
        .collect()
        .context("Failed to capture initial telemetry snapshot")?;

    let (snap_tx, snap_rx) = bounded::<SystemSnapshot>(2);
    let (cmd_tx, cmd_rx) = bounded::<ControlCmd>(8);
    let (ack_tx, ack_rx) = bounded::<String>(8);

    let sampler_handle = thread::spawn(move || {
        let ticker = tick(Duration::from_millis(750));
        loop {
            select! {
                recv(ticker) -> _ => {
                    match collector.collect() {
                        Ok(snap) => { let _ = snap_tx.try_send(snap); }
                        Err(e) => { let _ = ack_tx.try_send(format!("Telemetry warn: {:#}", e)); }
                    }
                }
                recv(cmd_rx) -> msg => {
                    match msg {
                        Ok(ControlCmd::Kill(pid)) => {
                            let res = match collector.kill_process(pid) {
                                Ok(()) => format!("Terminate signal sent to PID {}", pid),
                                Err(e) => format!("{:#}", e),
                            };
                            let _ = ack_tx.try_send(res);
                        }
                        Ok(ControlCmd::Shutdown) | Err(_) => break,
                    }
                }
            }
        }
    });

    enable_raw_mode().context("Failed to enable terminal raw mode")?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen).context("Failed to enter alternate screen")?;
    let mut terminal =
        Terminal::new(CrosstermBackend::new(stdout)).context("Failed to create TUI terminal")?;

    let mut current_snap = initial_snap;
    let mut ui_state = UiState::default();

    let app_res: Result<()> = loop {
        while let Ok(new_snap) = snap_rx.try_recv() {
            ui_state
                .cpu_history
                .push(new_snap.global_cpu_pct.round() as u64);
            if let Some(gpu) = &new_snap.gpu {
                ui_state.gpu_history.push(gpu.util_pct as u64);
            }
            current_snap = new_snap;
        }
        while let Ok(msg) = ack_rx.try_recv() {
            ui_state.status_banner = msg;
        }

        match ui_state.sort_by {
            SortField::Cpu => current_snap.processes.sort_by(|a, b| {
                b.cpu_pct
                    .partial_cmp(&a.cpu_pct)
                    .unwrap_or(std::cmp::Ordering::Equal)
            }),
            SortField::Memory => current_snap.processes.sort_by_key(|p| Reverse(p.rss_bytes)),
            SortField::Threads => current_snap.processes.sort_by_key(|p| Reverse(p.threads)),
        }

        terminal
            .draw(|f| render_dashboard(f, &current_snap, &mut ui_state))
            .context("TUI frame draw failed")?;

        if event::poll(Duration::from_millis(50)).context("Terminal event poll failed")?
            && let Event::Key(key) = event::read().context("Terminal event read failed")?
            && key.kind == KeyEventKind::Press
        {
            if ui_state.filter_active {
                match key.code {
                    KeyCode::Enter => {
                        ui_state.filter_active = false;
                        ui_state.filter_before_edit = None;
                    }
                    KeyCode::Esc => {
                        if let Some(previous) = ui_state.filter_before_edit.take() {
                            ui_state.process_filter = previous;
                        }
                        ui_state.filter_active = false;
                        ui_state.table_state.select(Some(0));
                    }
                    KeyCode::Backspace => {
                        ui_state.process_filter.pop();
                        ui_state.table_state.select(Some(0));
                    }
                    KeyCode::Char(ch) => {
                        ui_state.process_filter.push(ch);
                        ui_state.table_state.select(Some(0));
                    }
                    _ => {}
                }
            } else if let Some(pid) = ui_state.pending_kill {
                match key.code {
                    KeyCode::Char('y') | KeyCode::Char('Y') => {
                        let _ = cmd_tx.try_send(ControlCmd::Kill(pid));
                        ui_state.pending_kill = None;
                    }
                    KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                        ui_state.pending_kill = None;
                    }
                    _ => {}
                }
            } else {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break Ok(()),
                    KeyCode::Char('/') => {
                        ui_state.filter_before_edit = Some(ui_state.process_filter.clone());
                        ui_state.filter_active = true;
                    }
                    KeyCode::Char('x') => {
                        ui_state.process_filter.clear();
                        ui_state.table_state.select(Some(0));
                    }
                    KeyCode::Char('c') => ui_state.sort_by = SortField::Cpu,
                    KeyCode::Char('m') => ui_state.sort_by = SortField::Memory,
                    KeyCode::Char('t') => ui_state.sort_by = SortField::Threads,
                    KeyCode::Down | KeyCode::Char('j') => {
                        let i = ui_state.table_state.selected().unwrap_or(0);
                        let max_idx = filtered_processes(&current_snap, &ui_state.process_filter)
                            .len()
                            .saturating_sub(1);
                        ui_state.table_state.select(Some((i + 1).min(max_idx)));
                    }
                    KeyCode::Up => {
                        let i = ui_state.table_state.selected().unwrap_or(0);
                        ui_state.table_state.select(Some(i.saturating_sub(1)));
                    }
                    KeyCode::Char('k') => {
                        let selected = filtered_processes(&current_snap, &ui_state.process_filter)
                            .get(ui_state.table_state.selected().unwrap_or(0))
                            .copied();
                        if let Some(proc_item) = selected {
                            ui_state.pending_kill = Some(proc_item.pid);
                        }
                    }
                    _ => {}
                }
            }
        }
    };

    let _ = cmd_tx.send(ControlCmd::Shutdown);
    let _ = sampler_handle.join();
    restore_terminal();
    let _ = terminal.show_cursor();

    app_res
}
