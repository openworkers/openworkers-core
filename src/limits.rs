/// Runtime resource limits configuration
#[derive(Debug, Clone)]
pub struct RuntimeLimits {
    /// Initial heap size in MB (default: 1MB)
    pub heap_initial_mb: usize,
    /// Maximum heap size in MB (default: 128MB)
    pub heap_max_mb: usize,
    /// Maximum CPU time in milliseconds (default: 50ms, 0 = disabled)
    /// Only actual computation counts, sleeps/I/O don't count.
    pub max_cpu_time_ms: u64,
    /// Maximum wall-clock time in milliseconds (default: 30s, 0 = disabled)
    /// Total elapsed time including I/O. Prevents hanging on slow external APIs.
    pub max_wall_clock_time_ms: u64,
}

impl Default for RuntimeLimits {
    fn default() -> Self {
        Self {
            heap_initial_mb: 1,
            heap_max_mb: 128,
            max_cpu_time_ms: 50,            // 50ms CPU (anti-DDoS)
            max_wall_clock_time_ms: 30_000, // 30s total (anti-hang)
        }
    }
}
