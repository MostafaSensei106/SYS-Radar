mod footer;
mod header;
mod metrics;
mod processes;
mod system;

pub use footer::render_footer;
pub use header::render_header;
pub use metrics::render_metrics;
pub use processes::render_kill_confirmation;
pub use processes::render_processes;
pub use system::render_system;
