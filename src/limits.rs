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
    /// Stream buffer size for response body channels (default: 16)
    /// Higher values allow larger JS-generated streams without deadlock.
    /// Lower values save memory but may cause timeout on large streams.
    /// Memory usage: ~buffer_size × chunk_size (typically 1KB-64KB per chunk)
    pub stream_buffer_size: usize,
}

impl Default for RuntimeLimits {
    fn default() -> Self {
        Self {
            heap_initial_mb: 1,
            heap_max_mb: 128,
            max_cpu_time_ms: 50,            // 50ms CPU (anti-DDoS)
            max_wall_clock_time_ms: 30_000, // 30s total (anti-hang)
            stream_buffer_size: 16,         // 16 chunks buffer (conservative default)
        }
    }
}
