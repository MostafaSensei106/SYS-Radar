use std::collections::VecDeque;

#[derive(Debug, Clone)]
pub struct RingHistory<const T: usize> {
    buf: VecDeque<u64>,
}

impl<const T: usize> Default for RingHistory<T> {
    fn default() -> Self {
        Self {
            buf: VecDeque::with_capacity(T),
        }
    }
}

impl<const T: usize> RingHistory<T> {
    #[inline]
    pub fn push(&mut self, value: u64) {
        if self.buf.len() == T {
            self.buf.pop_front();
        }

        self.buf.push_back(value);
    }

    #[inline]
    pub fn to_vec(&self) -> Vec<u64> {
        self.buf.iter().copied().collect()
    }
}

// ** ------------------------------------------------------------- **
#[derive(Debug, Clone)]
pub struct CoreTelemetry {
    pub id: usize,
    pub usage_pct: f64,
    pub freq_mhz: u64,
}

// ** ------------------------------------------------------------- **
#[derive(Debug, Clone)]
pub struct MemoryTelemetry {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub cached_bytes: u64,
    pub swap_total_bytes: u64,
    pub swap_used_bytes: u64,
}

// ** ------------------------------------------------------------- **
#[derive(Debug, Clone)]
pub struct DiskTelemetry {
    pub mount: String,
    pub device: String,
    pub fs_type: String,
    pub total_bytes: u64,
    pub free_bytes: u64,
    pub read_bytes_per_sec: u64,
    pub write_bytes_per_sec: u64,
}

// ** ------------------------------------------------------------- **
#[derive(Debug, Clone, Copy, Default)]
pub struct NetTelemetry {
    pub rx_bytes_per_sec: u64,
    pub tx_bytes_per_sec: u64,
}

// ** ------------------------------------------------------------- **
#[derive(Debug, Clone)]
pub struct GpuTelemetry {
    pub name: String,
    pub util_pct: u32,
    pub mem_used: u64,
    pub mem_total: u64,
    pub temp_c: u32,
    pub power_mw: u32,
    pub clock_graphics_mhz: u32,
}

// ** ------------------------------------------------------------- **
#[derive(Debug, Clone)]
pub struct ProcessTelemetry {
    pub pid: u32,
    pub name: String,
    pub state: char,
    pub cpu_pct: f64,
    pub rss_bytes: u64,
    pub threads: u32,
    pub gpu_util_pct: Option<u32>,
    pub gpu_mem_bytes: Option<u64>,
}

// ** ------------------------------------------------------------- **
#[derive(Debug, Clone)]
pub struct HostStaticSpecs {
    pub hostname: String,
    pub os_name: String,
    pub kernel_version: String,
    pub arch: &'static str,
    pub cpu_model: String,
    pub physical_cores: usize,
    pub logical_cores: usize,
    pub desktop_env: String,
    pub terminal_name: String,
    pub shell_name: String,
}

// ** ------------------------------------------------------------- **
#[derive(Debug, Clone)]
pub struct SystemSnapshot {
    pub specs: HostStaticSpecs,
    pub global_cpu_pct: f64,
    pub cores: Vec<CoreTelemetry>,
    pub memory: MemoryTelemetry,
    pub gpu: Option<GpuTelemetry>,
    pub disks: Vec<DiskTelemetry>,
    pub net: NetTelemetry,
    pub processes: Vec<ProcessTelemetry>,
    pub uptime_secs: u64,
}
