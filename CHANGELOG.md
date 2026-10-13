# Changelog

Releases before 0.15.0 are only in the git log.

## [0.18.0] - 2026-10-13

### Added

- Durable Object binding: `BindingType::DurableObject` and
  `BindingInfo::durable_object()`. `Operation::BindingDurableObject` goes to
  `OperationsHandler::handle_binding_durable_object`, which answers an
  `ObjectResponse`. `durable_object_id(binding, ObjectIdSource)` makes the
  id of an object synchronously, so the runtime never holds the key of a
  namespace.
- Storage of the object a runtime serves: `ObjectStorageOp` (`Get`, `List`,
  `Write`, `GetAlarm`, `SetAlarm`) and `ObjectStorageResult`.
  `Operation::DurableObjectStorage` goes to `handle_durable_object_storage`.
  Values are bytes that the runtime serializes.
- WebSockets of the object, which the runner holds: `ObjectSocketOp`
  (`Accept`, `Attach`, `Send`, `Attachment`), `Operation::DurableObjectSocket`
  to `handle_durable_object_socket`, and `durable_object_sockets()` for the
  hibernatable sockets a new instance finds. `OperationResult::Upgrade`
  carries a 101 of an object with the sender for the socket of the client.

### Changed

- `Operation` and `OperationResult` have new variants; an exhaustive match
  needs an arm for each.
- A `WebSocketId` is unique in the process, not in one execution.

## [0.17.0] - 2026-10-12

### Added

- `FetchInit::upgradable` gives a fetch that the runner can upgrade to a
  WebSocket. It returns a sender: the runner sends the socket of the client
  on it after the 101, or drops it when the upgrade fails. The receiver is
  the new type `WebSocketUpgrade`.

### Changed

- `FetchInit` has a new public field, `upgrade: Option<WebSocketUpgrade>`.
  Code that builds a `FetchInit` with a struct literal must set it.
  `FetchInit::new` sets `None`.

## [0.16.1] - 2026-10-12

### Fixed

- The request conversions (`from_hyper_parts`, `from_hyper_parts_streaming`,
  `from_actix`) keep every value of a repeated header. They used to keep only
  the last one, so a browser that sends each cookie in its own header over
  HTTP/2 reached the worker with a single cookie. Repeated values are joined
  as the Fetch standard does: `"; "` for `cookie`, `", "` otherwise.

## [0.16.0] - 2026-10-12

### Added

- D1 binding, a SQLite database in the runner: `BindingType::D1`,
  `BindingInfo::d1()`, `D1Op` and `D1Result`. `Operation::BindingD1` goes to
  `OperationsHandler::handle_binding_d1`. The default stub returns an
  `Unavailable` error. A parameter is a `SqlPrimitive`, never an array. A
  statement result keeps the column names apart from the row values, so
  duplicate names stay. `docs/d1.md` maps the wire format to the Cloudflare
  D1 Worker API.
- `SqlPrimitive` and `SqlParam` deserialize a native byte string
  (`visit_bytes`, `visit_byte_buf`) as `Bytes`. Thus serde_v8 can send an
  `ArrayBuffer` or a typed array without base64.

### Changed

- `BindingType` has a new variant, `D1`. Add a `D1` arm to each exhaustive
  `match` on `BindingType`.

## [0.15.1] - 2026-09-13

### Changed

- Update base64 to 0.23.

## [0.15.0] - 2026-08-20

### Added

- `SqlPrimitive::Bytes(Vec<u8>)` for binary SQL values (Postgres `bytea`, D1
  blobs). JSON form is `{"$bytes": "<base64>"}` (standard alphabet, padded on
  encode, padding optional on decode); a JSON array of integers still
  deserializes as `SqlParam::Array`, never as bytes.
- `DatabaseResult::Table { columns, rows }`, a typed alternative to
  `Rows(String)` carrying `SqlPrimitive` values, so blobs reach the runtime
  without a JSON re-parse. Core does not convert between the two shapes.
- Images binding: `Operation::BindingImages` dispatched to
  `OperationsHandler::handle_binding_images` (the default stub errors),
  declared with `BindingType::Images` / `BindingInfo::images()`. Core
  transports the descriptor, the handler validates it. Every type but
  `ImagesResult` derives serde, so runtimes share one JS descriptor format;
  `ImageFormat::content_type()` maps a format to its MIME type. The covered
  subset matches the Cloudflare Images binding: the same five `fit` values
  with the same default, a crop expressed as pixels trimmed per edge, and a
  format accepted as `"image/webp"` as well as `"webp"`.
- `cron: Option<String>` on `TaskSource::Schedule`, skipped when absent, so a
  0.14 payload still deserializes.

### Changed

- `SqlPrimitive` and `SqlParam` deserialize through hand-written visitors
  instead of `#[serde(untagged)]`. Accepted shapes are unchanged; an invalid
  base64 payload now reports itself instead of "did not match any variant".
- `OperationResult` and `WebSocketConnection` derive `Debug`.
- New dependency: `base64`.

### Fixed

- `RequestBody::collect()` and `ResponseBody::collect()` dropped the failed
  chunks of a stream, so a truncated body looked like a complete one. They
  now stop at the first failure.

### Known gaps

- `RuntimeLimits` has no images entry; the right unit (operations, megapixels,
  CPU time) will come out of the first handler implementation.

### Breaking

- Exhaustive matches gain a variant: `Operation` (`BindingImages`),
  `OperationResult` (`Images`), `DatabaseResult` (`Table`), `SqlPrimitive`
  (`Bytes`), `BindingType` (`Images`).
- `TaskSource::Schedule { time }` patterns must become
  `TaskSource::Schedule { time, .. }` or bind the new `cron` field.
- `ImageTransform` and `ImageFormat` are `#[non_exhaustive]`, so catching up
  with Cloudflare transform by transform stays a minor version. A handler
  needs a catch-all arm returning an error that names what it does not
  support.
- `RequestBody::collect()` and `ResponseBody::collect()` return
  `Result<Option<Bytes>, String>` instead of `Option<Bytes>`.
- The `deno` feature is gone, along with the `deno_core::Resource` impls and
  the optional `deno_core` dependency.
