// Path: src/hal/linux.rs
use std::{
    collections::{HashMap, HashSet},
    env,
    ffi::CString,
    fs::{self, File},
    io::Read,
    mem::MaybeUninit,
    path::Path,
    time::Instant,
};

use anyhow::{Context, Result};
use nvml_wrapper::{
    Nvml,
    enum_wrappers::device::{Clock, TemperatureSensor},
};

use super::{HalError, HardwareCollector, linux_paths::KernelPaths};
use crate::domain::telemetry::*;

#[derive(Clone, Copy, Default)]
struct CpuTimes {
    user: u64,
    nice: u64,
    system: u64,
    idle: u64,
    iowait: u64,
    irq: u64,
    softirq: u64,
    steal: u64,
}

impl CpuTimes {
    #[inline]
    fn total(self) -> u64 {
        self.user
            + self.nice
            + self.system
            + self.idle
            + self.iowait
            + self.irq
            + self.softirq
            + self.steal
    }

    #[inline]
    fn active(self) -> u64 {
        self.total().saturating_sub(self.idle + self.iowait)
    }
}

pub struct LinuxProcCollector {
    paths: KernelPaths,
    specs: HostStaticSpecs,
    nvml: Option<Nvml>,
    prev_global_cpu: CpuTimes,
    prev_cores: Vec<CpuTimes>,
    prev_proc_ticks: HashMap<u32, u64>,
    prev_net: (u64, u64),
    prev_disk_io: HashMap<String, (u64, u64)>,
    last_sample: Instant,
    page_size: u64,
    scratch_buf: String,
    seen_devices: HashSet<String>,
}

impl LinuxProcCollector {
    pub fn new() -> Result<Self> {
        Self::with_paths(KernelPaths::default())
    }

    pub fn with_paths(paths: KernelPaths) -> Result<Self> {
        let page_size_raw = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
        if page_size_raw <= 0 {
            return Err(HalError::Syscall {
                syscall: "sysconf(_SC_PAGESIZE)",
                source: std::io::Error::last_os_error(),
            }
            .into());
        }
        let page_size = page_size_raw as u64;

        let mut scratch_buf = String::with_capacity(16 * 1024);

        let (global_cpu, cores) = Self::read_proc_stat(&paths.proc_stat, &mut scratch_buf)?;
        let net = Self::read_proc_net_dev(&paths.proc_net_dev, &mut scratch_buf)
            .context("Failed initial /proc/net/dev read")?;
        let disk_io = Self::read_diskstats(&paths.proc_diskstats, &mut scratch_buf)
            .context("Failed initial /proc/diskstats read")?;

        let hostname = Self::read_trimmed_or(&paths.proc_hostname, "linux");
        let kernel_version = Self::read_trimmed_or(&paths.proc_osrelease, "unknown");

        let os_name = fs::read_to_string(&paths.etc_os_release)
            .ok()
            .and_then(|content| {
                content
                    .lines()
                    .find(|l| l.starts_with("PRETTY_NAME="))
                    .map(|l| {
                        l.trim_start_matches("PRETTY_NAME=")
                            .trim_matches('"')
                            .to_string()
                    })
            })
            .unwrap_or_else(|| "Linux".into());

        let cpuinfo = fs::read_to_string(&paths.proc_cpuinfo).unwrap_or_default();
        let cpu_model = cpuinfo
            .lines()
            .find(|l| l.starts_with("model name"))
            .and_then(|l| l.split(':').nth(1))
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| "Unknown CPU".into());

        let logical_cores = cores.len().max(1);
        let mut unique_core_ids = HashSet::with_capacity(logical_cores);
        for line in cpuinfo.lines() {
            if line.starts_with("core id")
                && let Some((_, val)) = line.split_once(':')
                && let Ok(id) = val.trim().parse::<u32>()
            {
                unique_core_ids.insert(id);
            }
        }
        let physical_cores = if unique_core_ids.is_empty() {
            logical_cores
        } else {
            unique_core_ids.len()
        };

        let shell = env::var("SHELL").unwrap_or_else(|_| "sh".into());
        let shell_name = shell.rsplit('/').next().unwrap_or(&shell).to_string();
        let terminal_name = env::var("TERM_PROGRAM")
            .or_else(|_| env::var("TERM"))
            .unwrap_or_else(|_| "tty".into());
        let desktop_env = env::var("XDG_CURRENT_DESKTOP")
            .or_else(|_| env::var("DESKTOP_SESSION"))
            .unwrap_or_else(|_| "Wayland/X11".into());

        Ok(Self {
            paths,
            specs: HostStaticSpecs {
                hostname,
                os_name,
                kernel_version,
                arch: env::consts::ARCH,
                cpu_model,
                physical_cores,
                logical_cores,
                desktop_env,
                terminal_name,
                shell_name,
            },
            nvml: Nvml::init().ok(),
            prev_global_cpu: global_cpu,
            prev_cores: cores,
            prev_proc_ticks: HashMap::with_capacity(512),
            prev_net: net,
            prev_disk_io: disk_io,
            last_sample: Instant::now(),
            page_size,
            scratch_buf,
            seen_devices: HashSet::with_capacity(16),
        })
    }

    fn read_into_buf(path: &Path, buf: &mut String) -> std::result::Result<(), HalError> {
        buf.clear();
        let mut file = File::open(path).map_err(|source| HalError::FsRead {
            path: path.display().to_string(),
            source,
        })?;
        file.read_to_string(buf)
            .map_err(|source| HalError::FsRead {
                path: path.display().to_string(),
                source,
            })?;
        Ok(())
    }

    fn read_trimmed_or(path: &Path, fallback: &str) -> String {
        fs::read_to_string(path)
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|_| fallback.to_string())
    }

    fn read_proc_stat(
        path: &Path,
        buf: &mut String,
    ) -> std::result::Result<(CpuTimes, Vec<CpuTimes>), HalError> {
        Self::read_into_buf(path, buf)?;
        let mut global = None;
        let mut cores = Vec::with_capacity(32);

        for line in buf.lines() {
            if let Some(rest) = line.strip_prefix("cpu ") {
                global = Some(Self::parse_cpu_line(rest));
            } else if line.starts_with("cpu") {
                if let Some((_, rest)) = line.split_once(' ') {
                    cores.push(Self::parse_cpu_line(rest));
                }
            } else {
                break;
            }
        }

        let global = global.ok_or_else(|| HalError::Parse {
            path: path.display().to_string(),
            reason: "Missing aggregate 'cpu ' line",
        })?;

        Ok((global, cores))
    }

    #[inline]
    fn parse_cpu_line(s: &str) -> CpuTimes {
        let mut nums = s.split_whitespace().filter_map(|v| v.parse::<u64>().ok());
        CpuTimes {
            user: nums.next().unwrap_or(0),
            nice: nums.next().unwrap_or(0),
            system: nums.next().unwrap_or(0),
            idle: nums.next().unwrap_or(0),
            iowait: nums.next().unwrap_or(0),
            irq: nums.next().unwrap_or(0),
            softirq: nums.next().unwrap_or(0),
            steal: nums.next().unwrap_or(0),
        }
    }

    fn read_core_freqs(&self, count: usize) -> Vec<u64> {
        (0..count)
            .map(|i| {
                let path = self.paths.cpu_cur_freq(i);
                fs::read_to_string(path)
                    .ok()
                    .and_then(|s| s.trim().parse::<u64>().ok())
                    .map(|khz| khz / 1000)
                    .unwrap_or(0)
            })
            .collect()
    }

    fn read_meminfo(
        path: &Path,
        buf: &mut String,
    ) -> std::result::Result<MemoryTelemetry, HalError> {
        Self::read_into_buf(path, buf)?;
        let (mut total, mut avail, mut cached, mut buffers, mut swap_total, mut swap_free) =
            (0u64, 0u64, 0u64, 0u64, 0u64, 0u64);

        for line in buf.lines() {
            let Some((key, rest)) = line.split_once(':') else {
                continue;
            };
            let bytes = rest
                .split_whitespace()
                .next()
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(0)
                * 1024;

            match key {
                "MemTotal" => total = bytes,
                "MemAvailable" => avail = bytes,
                "Cached" => cached = bytes,
                "Buffers" => buffers = bytes,
                "SwapTotal" => swap_total = bytes,
                "SwapFree" => swap_free = bytes,
                _ => {}
            }
        }

        if total == 0 {
            return Err(HalError::Parse {
                path: path.display().to_string(),
                reason: "MemTotal missing or zero",
            });
        }

        Ok(MemoryTelemetry {
            total_bytes: total,
            used_bytes: total.saturating_sub(avail),
            cached_bytes: cached + buffers,
            swap_total_bytes: swap_total,
            swap_used_bytes: swap_total.saturating_sub(swap_free),
        })
    }

    fn read_proc_net_dev(
        path: &Path,
        buf: &mut String,
    ) -> std::result::Result<(u64, u64), HalError> {
        Self::read_into_buf(path, buf)?;
        let (mut rx_sum, mut tx_sum) = (0u64, 0u64);

        for line in buf.lines().skip(2) {
            let Some((iface, stats)) = line.split_once(':') else {
                continue;
            };
            if iface.trim() == "lo" {
                continue;
            }
            let mut cols = stats
                .split_whitespace()
                .filter_map(|v| v.parse::<u64>().ok());
            let rx = cols.next().unwrap_or(0);
            // تخطي الأعمدة 1..7 للوصول للعمود 8 (tx_bytes)
            let tx = cols.nth(7).unwrap_or(0);
            rx_sum += rx;
            tx_sum += tx;
        }
        Ok((rx_sum, tx_sum))
    }

    fn read_diskstats(
        path: &Path,
        buf: &mut String,
    ) -> std::result::Result<HashMap<String, (u64, u64)>, HalError> {
        Self::read_into_buf(path, buf)?;
        let mut map = HashMap::with_capacity(16);

        for line in buf.lines() {
            let mut cols = line.split_whitespace();
            let Some(dev) = cols.nth(2) else { continue };
            let sectors_read = cols.nth(2).and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
            let sectors_written = cols.nth(3).and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
            map.insert(dev.to_string(), (sectors_read * 512, sectors_written * 512));
        }
        Ok(map)
    }

    fn read_mounts(&mut self, dt_secs: f64) -> Result<Vec<DiskTelemetry>> {
        let curr_io = Self::read_diskstats(&self.paths.proc_diskstats, &mut self.scratch_buf)
            .context("Failed reading diskstats during mount scan")?;
        Self::read_into_buf(&self.paths.proc_mounts, &mut self.scratch_buf)?;

        self.seen_devices.clear();
        let mut out = Vec::with_capacity(8);

        for line in self.scratch_buf.lines() {
            let mut cols = line.split_whitespace();
            let (Some(dev), Some(mount), Some(fs_type)) = (cols.next(), cols.next(), cols.next())
            else {
                continue;
            };

            if !dev.starts_with("/dev/") || self.seen_devices.contains(dev) {
                continue;
            }
            self.seen_devices.insert(dev.to_string());

            let Ok(c_mount) = CString::new(mount) else {
                continue;
            };
            let mut stat = MaybeUninit::<libc::statvfs>::uninit();
            let rc = unsafe { libc::statvfs(c_mount.as_ptr(), stat.as_mut_ptr()) };
            if rc != 0 {
                continue;
            }

            let s = unsafe { stat.assume_init() };
            let total = s.f_blocks * s.f_frsize;
            let free = s.f_bavail * s.f_frsize;
            if total == 0 {
                continue;
            }

            let short_dev = dev.trim_start_matches("/dev/");
            let (r_rate, w_rate) = if let (Some(&(cr, cw)), Some(&(pr, pw))) =
                (curr_io.get(short_dev), self.prev_disk_io.get(short_dev))
            {
                (
                    (cr.saturating_sub(pr) as f64 / dt_secs) as u64,
                    (cw.saturating_sub(pw) as f64 / dt_secs) as u64,
                )
            } else {
                (0, 0)
            };

            out.push(DiskTelemetry {
                mount: mount.to_string(),
                device: dev.to_string(),
                fs_type: fs_type.to_string(),
                total_bytes: total,
                free_bytes: free,
                read_bytes_per_sec: r_rate,
                write_bytes_per_sec: w_rate,
            });
        }

        self.prev_disk_io = curr_io;
        Ok(out)
    }

    fn read_processes(&mut self, total_cpu_delta: u64) -> Result<Vec<ProcessTelemetry>> {
        let mut next_ticks = HashMap::with_capacity(self.prev_proc_ticks.len());
        let mut list = Vec::with_capacity(256);

        let entries = fs::read_dir(&self.paths.proc_root).map_err(|source| HalError::FsRead {
            path: self.paths.proc_root.display().to_string(),
            source,
        })?;

        for entry in entries.flatten() {
            let file_name = entry.file_name();
            let Some(pid_str) = file_name.to_str() else {
                continue;
            };
            let Ok(pid) = pid_str.parse::<u32>() else {
                continue;
            };

            let stat_path = self.paths.pid_stat(pid);
            if Self::read_into_buf(&stat_path, &mut self.scratch_buf).is_err() {
                continue;
            }

            let Some(open_paren) = self.scratch_buf.find('(') else {
                continue;
            };
            let Some(close_paren) = self.scratch_buf.rfind(')') else {
                continue;
            };
            if close_paren <= open_paren || close_paren + 2 >= self.scratch_buf.len() {
                continue;
            }

            let name = self.scratch_buf[open_paren + 1..close_paren].to_string();
            let mut fields = self.scratch_buf[close_paren + 2..].split_whitespace();

            let state = fields.next().and_then(|s| s.chars().next()).unwrap_or('?');
            let utime = fields
                .nth(10)
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(0);
            let stime = fields
                .next()
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(0);
            let threads = fields
                .nth(4)
                .and_then(|v| v.parse::<u32>().ok())
                .unwrap_or(1);
            let rss_pages = fields
                .nth(3)
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(0);

            let proc_ticks = utime + stime;
            next_ticks.insert(pid, proc_ticks);

            let prev_ticks = self
                .prev_proc_ticks
                .get(&pid)
                .copied()
                .unwrap_or(proc_ticks);
            let tick_delta = proc_ticks.saturating_sub(prev_ticks);
            let cpu_pct = if total_cpu_delta > 0 {
                (tick_delta as f64 / total_cpu_delta as f64)
                    * 100.0
                    * (self.specs.logical_cores as f64)
            } else {
                0.0
            };

            list.push(ProcessTelemetry {
                pid,
                name,
                state,
                cpu_pct,
                rss_bytes: rss_pages * self.page_size,
                threads,
            });
        }

        self.prev_proc_ticks = next_ticks;
        Ok(list)
    }

    fn read_gpu(&self) -> Option<GpuTelemetry> {
        let nvml = self.nvml.as_ref()?;
        let dev = nvml.device_by_index(0).ok()?;
        let util = dev.utilization_rates().ok()?;
        let mem = dev.memory_info().ok()?;
        Some(GpuTelemetry {
            name: dev.name().unwrap_or_else(|_| "NVIDIA GPU".into()),
            util_pct: util.gpu,
            mem_used: mem.used,
            mem_total: mem.total,
            temp_c: dev.temperature(TemperatureSensor::Gpu).unwrap_or(0),
            power_mw: dev.power_usage().unwrap_or(0),
            clock_graphics_mhz: dev.clock_info(Clock::Graphics).unwrap_or(0),
        })
    }
}
impl HardwareCollector for LinuxProcCollector {
    fn static_soecs(&self) -> &HostStaticSpecs {
        &self.specs
    }

    fn collect(&mut self) -> std::result::Result<SystemSnapshot, HalError> {
        let now = Instant::now();
        let dt_secs = now.duration_since(self.last_sample).as_secs_f64().max(0.05);
        self.last_sample = now;

        let (curr_global, curr_cores) =
            Self::read_proc_stat(&self.paths.proc_stat, &mut self.scratch_buf)
                .context("Failed sampling /proc/stat")
                .unwrap();

        let total_delta = curr_global
            .total()
            .saturating_sub(self.prev_global_cpu.total());
        let active_delta = curr_global
            .active()
            .saturating_sub(self.prev_global_cpu.active());
        let global_cpu_pct = if total_delta > 0 {
            (active_delta as f64 / total_delta as f64) * 100.0
        } else {
            0.0
        };
        self.prev_global_cpu = curr_global;

        let freqs = self.read_core_freqs(curr_cores.len());
        let cores: Vec<CoreTelemetry> = curr_cores
            .iter()
            .enumerate()
            .map(|(idx, curr)| {
                let prev = self.prev_cores.get(idx).copied().unwrap_or_default();
                let t_d = curr.total().saturating_sub(prev.total());
                let a_d = curr.active().saturating_sub(prev.active());
                let pct = if t_d > 0 {
                    (a_d as f64 / t_d as f64) * 100.0
                } else {
                    0.0
                };
                CoreTelemetry {
                    id: idx,
                    usage_pct: pct,
                    freq_mhz: freqs.get(idx).copied().unwrap_or(0),
                }
            })
            .collect();
        self.prev_cores = curr_cores;

        let memory = Self::read_meminfo(&self.paths.proc_meminfo, &mut self.scratch_buf)
            .context("Failed sampling /proc/meminfo")
            .unwrap();

        let (rx, tx) = Self::read_proc_net_dev(&self.paths.proc_net_dev, &mut self.scratch_buf)
            .context("Failed sampling /proc/net/dev")
            .unwrap();
        let net = NetTelemetry {
            rx_bytes_per_sec: (rx.saturating_sub(self.prev_net.0) as f64 / dt_secs) as u64,
            tx_bytes_per_sec: (tx.saturating_sub(self.prev_net.1) as f64 / dt_secs) as u64,
        };
        self.prev_net = (rx, tx);

        let disks = self.read_mounts(dt_secs).unwrap();
        let processes = self.read_processes(total_delta).unwrap();

        let uptime_secs = Self::read_into_buf(&self.paths.proc_uptime, &mut self.scratch_buf)
            .ok()
            .and_then(|_| {
                self.scratch_buf
                    .split_whitespace()
                    .next()
                    .and_then(|v| v.parse::<f64>().ok())
            })
            .unwrap_or(0.0) as u64;

        Ok(SystemSnapshot {
            specs: self.specs.clone(),
            global_cpu_pct,
            cores,
            memory,
            gpu: self.read_gpu(),
            disks,
            net,
            processes,
            uptime_secs,
        })
    }

    fn kill_process(&self, pid: u32) -> std::result::Result<(), HalError> {
        let rc = unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) };
        if rc == 0 {
            Ok(())
        } else {
            Err(HalError::Syscall {
                syscall: "kill(SIGTERM)",
                source: std::io::Error::last_os_error(),
            })
        }
    }
}
