use thiserror::Error;

use crate::domain::telemetry::{HostStaticSpecs, SystemSnapshot};

#[derive(Debug, Error)]
pub enum HalError {
    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),

    #[error("Kernel/OS I/O failure on '{path}': {source}")]
    FsRead {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("Failed to create hardware collector: {detail}")]
    CreateCollector { detail: String },

    #[error("Malformed kernel telemetry in '{path}': {reason}")]
    Parse { path: String, reason: &'static str },

    #[error("OS syscall '{syscall}' failed: {source}")]
    Syscall {
        syscall: &'static str,
        #[source]
        source: std::io::Error,
    },

    #[error("Platform API '{api}' error: {detail}")]
    PlatformApi { api: &'static str, detail: String },

    #[error("Failed to kill process '{name}' (PID {pid}): {source}")]
    KillProcess {
        name: String,
        pid: u32,
        #[source]
        source: std::io::Error,
    },

    #[error("Unknown state: {state}")]
    UnknownState { state: String },
}

pub trait HardwareCollector: Send + 'static {
    fn static_soecs(&self) -> &HostStaticSpecs;
    fn collect(&mut self) -> Result<SystemSnapshot, HalError>;
    fn kill_process(&self, pid: u32) -> Result<(), HalError>;
}

#[cfg(target_os = "linux")]
pub mod linux;

#[cfg(target_os = "linux")]
pub mod linux_paths;

#[cfg(target_os = "windows")]
pub mod windows;

pub fn create_collector() -> Result<Box<dyn HardwareCollector>, HalError> {
    #[cfg(target_os = "linux")]
    {
        let collector = linux::LinuxProcCollector::new()?;
        Ok(Box::new(collector))
    }
    #[cfg(target_os = "windows")]
    {
        let collector = windows::WindowsWin32Collector::new()?;
        Ok(Box::new(collector))
    }
}
