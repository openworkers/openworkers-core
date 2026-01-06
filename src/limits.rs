/// Limit configuration for a specific binding (fetch, KV, database, etc.)
#[derive(Debug, Clone)]
pub struct BindingLimit {
    /// Maximum total calls per request (0 = unlimited)
    pub max_total: u32,
    /// Maximum concurrent calls (0 = unlimited)
    pub max_concurrent: u32,
}

impl BindingLimit {
    /// Create a new binding limit
    pub fn new(max_total: u32, max_concurrent: u32) -> Self {
        Self {
            max_total,
            max_concurrent,
        }
    }

    /// Unlimited (no restrictions)
    pub fn unlimited() -> Self {
        Self {
            max_total: 0,
            max_concurrent: 0,
        }
    }
}

impl Default for BindingLimit {
    fn default() -> Self {
        Self::unlimited()
    }
}

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

    /// Fetch (outbound HTTP) limit (default: 50 total, 6 concurrent)
    pub fetch_limit: BindingLimit,
    /// KV store limit (default: 1000 total, 10 concurrent)
    pub kv_limit: BindingLimit,
    /// Database query limit (default: 100 total, 5 concurrent)
    pub database_limit: BindingLimit,
    /// Storage (R2/S3) limit (default: 100 total, 3 concurrent)
    pub storage_limit: BindingLimit,
}

impl Default for RuntimeLimits {
    fn default() -> Self {
        Self {
            heap_initial_mb: 1,
            heap_max_mb: 128,
            max_cpu_time_ms: 50,            // 50ms CPU (anti-DDoS)
            max_wall_clock_time_ms: 30_000, // 30s total (anti-hang)
            stream_buffer_size: 16,         // 16 chunks buffer (conservative default)

            // Binding limits (conservative defaults, similar to Cloudflare free tier)
            fetch_limit: BindingLimit::new(50, 6), // 50 total, 6 concurrent (like browsers)
            kv_limit: BindingLimit::new(1000, 10), // 1000 total, 10 concurrent
            database_limit: BindingLimit::new(100, 5), // 100 total, 5 concurrent
            storage_limit: BindingLimit::new(100, 3), // 100 total, 3 concurrent
        }
    }
}
