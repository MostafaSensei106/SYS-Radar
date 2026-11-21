use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct KernelPaths {
    pub proc_stat: PathBuf,
    pub proc_meminfo: PathBuf,
    pub proc_net_dev: PathBuf,
    pub proc_diskstats: PathBuf,
    pub proc_mounts: PathBuf,
    pub proc_uptime: PathBuf,
    pub proc_cpuinfo: PathBuf,
    pub proc_hostname: PathBuf,
    pub proc_osrelease: PathBuf,
    pub proc_root: PathBuf,
    pub etc_os_release: PathBuf,
    pub sys_cpu_root: PathBuf,
}

impl KernelPaths {
    pub fn form_root(root: impl AsRef<Path>) -> Self {
        let r = root.as_ref();
        Self {
            proc_stat: r.join("proc/stat"),
            proc_meminfo: r.join("proc/meminfo"),
            proc_net_dev: r.join("proc/net/dev"),
            proc_diskstats: r.join("proc/diskstats"),
            proc_mounts: r.join("proc/mounts"),
            proc_uptime: r.join("proc/uptime"),
            proc_cpuinfo: r.join("proc/cpuinfo"),
            proc_hostname: r.join("proc/sys/kernel/hostname"),
            proc_osrelease: r.join("proc/sys/kernel/osrelease"),
            proc_root: r.join("proc"),
            etc_os_release: r.join("etc/os-release"),
            sys_cpu_root: r.join("sys/devices/system/cpu"),
        }
    }

    #[inline]
    pub fn pid_stat(&self, pid: u32) -> PathBuf {
        self.proc_root.join(pid.to_string()).join("stat")
    }

    #[inline]
    pub fn cpu_cur_freq(&self, core_id: usize) -> PathBuf {
        self.sys_cpu_root
            .join(format!("cpu{}", core_id))
            .join("cpufreq/scaling_cur_freq")
    }
}

impl Default for KernelPaths {
    fn default() -> Self {
        Self::form_root("/")
    }
}
