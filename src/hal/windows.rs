use std::{collections::HashMap, env, mem::size_of, time::Instant};

use anyhow::{Context, Result};
use nvml_wrapper::{
    Nvml,
    enum_wrappers::device::{Clock, TemperatureSensor},
};
use windows::Win32::{
    Foundation::{CloseHandle, FILETIME},
    NetworkManagement::IpHelper::{FreeMibTable, GetIfTable2, MIB_IF_TABLE2},
    Storage::FileSystem::{GetDiskFreeSpaceExW, GetLogicalDrives},
    System::{
        Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
            TH32CS_SNAPPROCESS,
        },
        ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS},
        SystemInformation::{
            GetSystemInfo, GetTickCount64, GlobalMemoryStatusEx, MEMORYSTATUSEX, SYSTEM_INFO,
        },
        Threading::{
            GetProcessTimes, GetSystemTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
            PROCESS_TERMINATE, TerminateProcess,
        },
    },
};

use super::{HalError, HardwareCollector};
use crate::domain::telemetry::*;

#[inline]
fn ft_to_u64(ft: FILETIME) -> u64 {
    ((ft.dwHighDateTime as u64) << 32) | (ft.dwLowDateTime as u64)
}

pub struct WindowsWin32Collector {
    specs: HostStaticSpecs,
    nvml: Option<Nvml>,
    prev_sys_times: (u64, u64, u64),
    prev_proc_times: HashMap<u32, u64>,
    prev_net: (u64, u64),
    last_sample: Instant,
}

impl WindowsWin32Collector {
    pub fn new() -> Result<Self> {
        let mut sys_info = SYSTEM_INFO::default();
        unsafe { GetSystemInfo(&mut sys_info) };
        let logical_cores = sys_info.dwNumberOfProcessors.max(1) as usize;

        let hostname = env::var("COMPUTERNAME").unwrap_or_else(|_| "WINDOWS-PC".into());
        let cpu_model = env::var("PROCESSOR_IDENTIFIER").unwrap_or_else(|_| "x86_64 CPU".into());

        let prev_sys_times =
            Self::read_sys_times().context("Failed initial GetSystemTimes call")?;
        let prev_net = Self::read_net_counters().context("Failed initial GetIfTable2 call")?;

        Ok(Self {
            specs: HostStaticSpecs {
                hostname,
                os_name: "Windows NT".into(),
                kernel_version: "Win32".into(),
                arch: env::consts::ARCH,
                cpu_model,
                physical_cores: logical_cores,
                logical_cores,
                desktop_env: "DWM / Explorer".into(),
                terminal_name: env::var("WT_SESSION")
                    .map(|_| "Windows Terminal".to_string())
                    .unwrap_or_else(|_| "Console".into()),
                shell_name: "pwsh/cmd".into(),
            },
            nvml: Nvml::init().ok(),
            prev_sys_times,
            prev_proc_times: HashMap::with_capacity(512),
            prev_net,
            last_sample: Instant::now(),
        })
    }

    fn read_sys_times() -> std::result::Result<(u64, u64, u64), HalError> {
        let (mut idle, mut kernel, mut user) = (
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
        );
        unsafe {
            GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user)).map_err(|e| {
                HalError::PlatformApi {
                    api: "GetSystemTimes",
                    detail: e.to_string(),
                }
            })?;
        }
        Ok((ft_to_u64(idle), ft_to_u64(kernel), ft_to_u64(user)))
    }

    fn read_net_counters() -> std::result::Result<(u64, u64), HalError> {
        let mut table_ptr: *mut MIB_IF_TABLE2 = std::ptr::null_mut();
        let (mut rx, mut tx) = (0u64, 0u64);
        unsafe {
            GetIfTable2(&mut table_ptr).map_err(|e| HalError::PlatformApi {
                api: "GetIfTable2",
                detail: e.to_string(),
            })?;
            if !table_ptr.is_null() {
                let table = &*table_ptr;
                let rows =
                    std::slice::from_raw_parts(table.Table.as_ptr(), table.NumEntries as usize);
                for row in rows {
                    if row.InterfaceAndOperStatusFlags._bitfield & 0x1 != 0 {
                        rx += row.InOctets;
                        tx += row.OutOctets;
                    }
                }
                FreeMibTable(table_ptr as *const _);
            }
        }
        Ok((rx, tx))
    }

    fn read_disks() -> Vec<DiskTelemetry> {
        let mask = unsafe { GetLogicalDrives() };
        let mut disks = Vec::with_capacity(8);

        for i in 0..26u32 {
            if (mask & (1 << i)) == 0 {
                continue;
            }
            let letter = (b'A' + i as u8) as char;
            let root = format!("{}:\\", letter);
            let wide: Vec<u16> = root.encode_utf16().chain(std::iter::once(0)).collect();
            let (mut free_avail, mut total, mut total_free) = (0u64, 0u64, 0u64);

            let ok = unsafe {
                GetDiskFreeSpaceExW(
                    windows::core::PCWSTR(wide.as_ptr()),
                    Some(&mut free_avail),
                    Some(&mut total),
                    Some(&mut total_free),
                )
                .is_ok()
            };

            if ok && total > 0 {
                disks.push(DiskTelemetry {
                    mount: root,
                    device: format!("Drive {}", letter),
                    fs_type: "NTFS".into(),
                    total_bytes: total,
                    free_bytes: free_avail,
                    read_bytes_per_sec: 0,
                    write_bytes_per_sec: 0,
                });
            }
        }
        disks
    }

    fn read_processes(&mut self, sys_total_delta: u64) -> Result<Vec<ProcessTelemetry>> {
        let mut list = Vec::with_capacity(256);
        let mut next_times = HashMap::with_capacity(self.prev_proc_times.len());

        let snap = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }.map_err(|e| {
            HalError::PlatformApi {
                api: "CreateToolhelp32Snapshot",
                detail: e.to_string(),
            }
        })?;

        let mut entry = PROCESSENTRY32W {
            dwSize: size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };

        if unsafe { Process32FirstW(snap, &mut entry).is_ok() } {
            loop {
                let pid = entry.th32ProcessID;
                let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(0);
                let name = String::from_utf16_lossy(&entry.szExeFile[..len]);

                let mut rss = 0u64;
                let mut cpu_pct = 0.0;

                if let Ok(handle) =
                    unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }
                {
                    let mut pmc = PROCESS_MEMORY_COUNTERS {
                        cb: size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
                        ..Default::default()
                    };
                    if unsafe {
                        GetProcessMemoryInfo(
                            handle,
                            &mut pmc,
                            size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
                        )
                        .is_ok()
                    } {
                        rss = pmc.WorkingSetSize as u64;
                    }

                    let (mut c, mut e, mut k, mut u) = (
                        FILETIME::default(),
                        FILETIME::default(),
                        FILETIME::default(),
                        FILETIME::default(),
                    );
                    if unsafe { GetProcessTimes(handle, &mut c, &mut e, &mut k, &mut u).is_ok() } {
                        let ticks = ft_to_u64(k) + ft_to_u64(u);
                        next_times.insert(pid, ticks);
                        let prev = self.prev_proc_times.get(&pid).copied().unwrap_or(ticks);
                        let delta = ticks.saturating_sub(prev);
                        if sys_total_delta > 0 {
                            cpu_pct = (delta as f64 / sys_total_delta as f64) * 100.0;
                        }
                    }
                    unsafe {
                        let _ = CloseHandle(handle);
                    }
                }

                list.push(ProcessTelemetry {
                    pid,
                    name,
                    state: 'R',
                    cpu_pct,
                    rss_bytes: rss,
                    threads: entry.cntThreads,
                });

                if unsafe { Process32NextW(snap, &mut entry).is_err() } {
                    break;
                }
            }
        }

        unsafe {
            let _ = CloseHandle(snap);
        }
        self.prev_proc_times = next_times;
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

impl HardwareCollector for WindowsWin32Collector {
    fn static_specs(&self) -> &HostStaticSpecs {
        &self.specs
    }

    fn collect(&mut self) -> Result<SystemSnapshot> {
        let now = Instant::now();
        let dt_secs = now.duration_since(self.last_sample).as_secs_f64().max(0.05);
        self.last_sample = now;

        let (idle, kernel, user) =
            Self::read_sys_times().context("Failed sampling GetSystemTimes")?;
        let idle_d = idle.saturating_sub(self.prev_sys_times.0);
        let kernel_d = kernel.saturating_sub(self.prev_sys_times.1);
        let user_d = user.saturating_sub(self.prev_sys_times.2);
        let total_d = kernel_d + user_d;
        let active_d = total_d.saturating_sub(idle_d);
        let global_cpu_pct = if total_d > 0 {
            (active_d as f64 / total_d as f64) * 100.0
        } else {
            0.0
        };
        self.prev_sys_times = (idle, kernel, user);

        let mut mem_stat = MEMORYSTATUSEX {
            dwLength: size_of::<MEMORYSTATUSEX>() as u32,
            ..Default::default()
        };
        unsafe {
            GlobalMemoryStatusEx(&mut mem_stat).map_err(|e| HalError::PlatformApi {
                api: "GlobalMemoryStatusEx",
                detail: e.to_string(),
            })?;
        }

        let (rx, tx) = Self::read_net_counters().context("Failed sampling GetIfTable2")?;
        let net = NetTelemetry {
            rx_bytes_per_sec: (rx.saturating_sub(self.prev_net.0) as f64 / dt_secs) as u64,
            tx_bytes_per_sec: (tx.saturating_sub(self.prev_net.1) as f64 / dt_secs) as u64,
        };
        self.prev_net = (rx, tx);

        let cores = (0..self.specs.logical_cores)
            .map(|id| CoreTelemetry {
                id,
                usage_pct: global_cpu_pct,
                freq_mhz: 0,
            })
            .collect();

        Ok(SystemSnapshot {
            specs: self.specs.clone(),
            global_cpu_pct,
            cores,
            memory: MemoryTelemetry {
                total_bytes: mem_stat.ullTotalPhys,
                used_bytes: mem_stat.ullTotalPhys.saturating_sub(mem_stat.ullAvailPhys),
                cached_bytes: 0,
                swap_total_bytes: mem_stat.ullTotalPageFile,
                swap_used_bytes: mem_stat
                    .ullTotalPageFile
                    .saturating_sub(mem_stat.ullAvailPageFile),
            },
            gpu: self.read_gpu(),
            disks: Self::read_disks(),
            net,
            processes: self.read_processes(total_d)?,
            uptime_secs: unsafe { GetTickCount64() } / 1000,
        })
    }

    fn kill_process(&self, pid: u32) -> Result<()> {
        unsafe {
            let handle =
                OpenProcess(PROCESS_TERMINATE, false, pid).map_err(|e| HalError::PlatformApi {
                    api: "OpenProcess(PROCESS_TERMINATE)",
                    detail: e.to_string(),
                })?;
            let res = TerminateProcess(handle, 1).map_err(|e| HalError::PlatformApi {
                api: "TerminateProcess",
                detail: e.to_string(),
            });
            let _ = CloseHandle(handle);
            res.with_context(|| format!("Failed to terminate PID {}", pid))
        }
    }
}
