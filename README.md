# openworkers-core

Shared types for OpenWorkers runtimes (V8, JSC, QuickJS, Boa, Nova, Wasmtime)
and for the runner that drives them.

## Installation

```toml
[dependencies]
openworkers-core = "0.15"
```

## Features

| Feature | Description |
|---------|-------------|
| `actix` | Actix-web request/response conversions |
| `hyper` | Hyper request/response conversions |
| `deno`  | `deno_core::Resource` impls for the event types |
| `wasm`  | `WorkerCode::WebAssembly` |

## Worker

A runtime implements `Worker` over a `Script`:

```rust
pub trait Worker: Sized {
    fn new(script: Script, limits: Option<RuntimeLimits>)
        -> impl Future<Output = Result<Self, TerminationReason>>;
    fn exec(&mut self, event: Event) -> impl Future<Output = Result<(), TerminationReason>>;
    fn abort(&mut self);
}
```

Futures are not `Send`: JS runtimes keep thread-local contexts.

`Script` carries the code, the environment variables and the binding names
(`BindingInfo`, no credentials):

```rust
let script = Script::new("export default { fetch: () => new Response('hi') }");
let script = Script::with_env(code, HashMap::from([("API_KEY".into(), "secret".into())]));
```

`WorkerCode` is `JavaScript(String)`, `Snapshot(Vec<u8>)`, or
`WebAssembly(Vec<u8>)` under the `wasm` feature.

## Events

`Event::Fetch` handles an HTTP request, `Event::Task` covers scheduled,
chained and manually invoked runs. Both constructors return the receiver for
the result:

```rust
let (event, res_rx) = Event::fetch(request);
let (event, res_rx) = Event::from_schedule(task_id, scheduled_time_ms);
let (event, res_rx) = Event::invoke(task_id, payload, Some("cli".into()));
```

`TaskSource` records what triggered a task: `Schedule { time, cron }`,
`Chained { .. }`, `Worker { .. }` or `Invoke { origin }`. A task answers with
`TaskResult`.

## Operations

Everything a worker asks of the outside world goes through
`OperationsHandler`, which the runner implements. Every method has a stub
default, so a runner overrides only what it supports:

```rust
impl OperationsHandler for MyRunner {
    fn handle_fetch(&self, request: HttpRequest) -> OpFuture<'_, Result<HttpResponse, String>> {
        Box::pin(async move { /* ... */ })
    }
}
```

| Operation | Handler | Result |
|-----------|---------|--------|
| `Fetch` | `handle_fetch` | `Http` |
| `BindingFetch` | `handle_binding_fetch` | `Http` |
| `BindingStorage` | `handle_binding_storage` | `Storage` |
| `BindingKv` | `handle_binding_kv` | `Kv` |
| `BindingDatabase` | `handle_binding_database` | `Database` |
| `BindingImages` | `handle_binding_images` | `Images` |
| `BindingWorker` | `handle_binding_worker` | `Http` |
| `WebSocketConnect` | `handle_websocket_connect` | `WebSocket` |
| `Log` | `handle_log` | `Ack` |

`handle` dispatches an `Operation` to these methods; override it only for
custom dispatch.

### Database

Query parameters are `SqlParam`: a `SqlPrimitive` or an array of them. JSON
has no byte string, so `SqlPrimitive::Bytes` is tagged; an array of integers
is always an array, never bytes:

```json
{"$bytes": "3q2+7w=="}
```

A handler answers with `DatabaseResult::Rows(String)` (rows already
serialized to JSON) or `DatabaseResult::Table { columns, rows }` (typed
values, which binary columns survive). Core does not convert between them.

### Images

`ImagesOp::Transform` is a descriptor: input bytes, transforms applied in
order, output encoding.

```json
{"transform": {
  "input": [137, 80, 78, 71],
  "transforms": [{"resize": {"width": 800, "fit": "cover"}}, {"rotate": {"degrees": 90}}],
  "output": {"format": "webp", "quality": 80}
}}
```

Core transports it as-is; the handler validates it and returns the encoded
image. The MIME type comes from `ImageFormat::content_type()`.

## Limits

`RuntimeLimits` caps heap, CPU time, wall-clock time and stream buffering, and
holds a `BindingLimit` (total and concurrent calls) per binding family.
Exceeding one ends the execution with a `TerminationReason`, which maps to an
HTTP status via `http_status()`.

## License

MIT
