use std::collections::HashMap;

use crate::domain::telemetry::{DiskTelemetry, RingHistory, SystemSnapshot};
use ratatui::widgets::TableState;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SortField {
    Cpu,
    Memory,
    Threads,
}

impl SortField {
    pub fn label(self) -> &'static str {
        match self {
            Self::Cpu => "CPU",
            Self::Memory => "MEMORY",
            Self::Threads => "THREADS",
        }
    }
}

pub struct UiState {
    pub sort_by: SortField,
    pub table_state: TableState,
    pub status_banner: String,
    pub cpu_history: RingHistory<60>,
    pub gpu_history: RingHistory<60>,
    pub storage_history: HashMap<String, RingHistory<30>>,
    pub process_filter: String,
    pub filter_active: bool,
    pub filter_before_edit: Option<String>,
    pub pending_kill: Option<u32>,
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
            storage_history: HashMap::new(),
            process_filter: String::new(),
            filter_active: false,
            filter_before_edit: None,
            pending_kill: None,
        }
    }
}

impl UiState {
    pub fn record_storage_sample(&mut self, disks: &[DiskTelemetry]) {
        for disk in disks {
            self.storage_history
                .entry(disk.mount.clone())
                .or_default()
                .push(
                    disk.read_bytes_per_sec
                        .saturating_add(disk.write_bytes_per_sec),
                );
        }
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

pub fn format_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    const GIB: f64 = MIB * 1024.0;
    let value = bytes as f64;
    if value >= GIB {
        format!("{:.1} GiB", value / GIB)
    } else if value >= MIB {
        format!("{:.0} MiB", value / MIB)
    } else if value >= KIB {
        format!("{:.0} KiB", value / KIB)
    } else {
        format!("{bytes} B")
    }
}
