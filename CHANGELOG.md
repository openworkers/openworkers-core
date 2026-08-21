# Changelog

Releases before 0.15.0 are only in the git log.

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
