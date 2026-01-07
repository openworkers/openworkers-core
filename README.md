# openworkers-core

Shared types for OpenWorkers runtimes.

This crate provides the common abstractions used across all runtime implementations (V8, Wasmtime, Deno, QuickJS, Boa).

## Installation

```toml
[dependencies]
openworkers-core = "0.8"
```

## Features

| Feature | Description |
|---------|-------------|
| `actix` | Actix-web request/response conversions |
| `hyper` | Hyper request/response conversions |
| `deno`  | Deno runtime integration |

## Core Types

### WorkerCode

Represents the executable code for a worker:

```rust
pub enum WorkerCode {
    JavaScript(String),   // JS/TS source code
    WebAssembly(Vec<u8>), // WASM component binary
    Snapshot(Vec<u8>),    // Pre-compiled snapshot
}
```

### Script

A worker script with code, environment variables, and bindings:

```rust
let script = Script::new("export default { fetch(req) { return new Response('Hello'); } }");

// Or with environment variables
let script = Script::with_env(code, HashMap::from([
    ("API_KEY".into(), "secret".into()),
]));
```

### RuntimeLimits

Resource limits for worker execution:

```rust
let limits = RuntimeLimits {
    heap_max_mb: 128,           // Max memory
    max_cpu_time_ms: 50,        // CPU time limit
    max_wall_clock_time_ms: 30_000, // Total time limit
    fetch_limit: BindingLimit::new(50, 6), // 50 total, 6 concurrent
    ..Default::default()
};
```

### HTTP Types

- `HttpRequest` / `HttpResponse` - Platform-agnostic HTTP types
- `RequestBody` - Buffered request body
- `ResponseBody` - Supports buffered or streaming responses

### Operations

Async operations for bindings (KV, Database, Storage):

```rust
pub enum Operation {
    Kv(KvOp),
    Database(DatabaseOp),
    Storage(StorageOp),
}
```

### Worker Trait

Common interface for all runtime implementations:

```rust
#[async_trait]
pub trait Worker: Sized {
    async fn new(script: Script, limits: Option<RuntimeLimits>) -> Result<Self, TerminationReason>;
    async fn exec(&mut self, task: Task) -> Result<(), TerminationReason>;
    fn abort(&mut self);
}
```

## License

MIT
