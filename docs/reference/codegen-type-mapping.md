# Codegen Type Mapping Reference

How `cargo nano-ros generate` maps ROS 2 message fields to Rust, C, and C++ types.

## Overview

Every ROS message generates **a single owned Rust struct** — no lifetime
parameters, no dual `Msg<'a>` / `MsgOwned` variants. Unbounded fields
(`string`, `uint8[]`, etc.) use fixed-capacity `heapless` types. All
generated types implement `Serialize`, `Deserialize`, `RosMessage`,
`Default`, and `Clone`.

For zero-copy access to large payloads (images, point clouds), use the
**raw subscription API** and read CDR fields directly with `CdrReader`
instead of fully deserializing into owned structs.

## Rust type mapping

### Primitive fields

| ROS type         | Rust type |
|------------------|-----------|
| `bool`           | `bool`    |
| `byte` / `uint8` | `u8`      |
| `char` / `int8`  | `i8`      |
| `int16`          | `i16`     |
| `uint16`         | `u16`     |
| `int32`          | `i32`     |
| `uint32`         | `u32`     |
| `int64`          | `i64`     |
| `uint64`         | `u64`     |
| `float32`        | `f32`     |
| `float64`        | `f64`     |

### String fields

| ROS type               | Rust type               | Notes                         |
|------------------------|-------------------------|-------------------------------|
| `string` (unbounded)   | `heapless::String<256>` | Capacity is the codegen limit |
| `wstring` (unbounded)  | `heapless::String<256>` | Same                          |
| `string<=N` (bounded)  | `heapless::String<N>`   | Capacity matches bound        |
| `wstring<=N` (bounded) | `heapless::String<N>`   | Same                          |

`heapless::String<N>` is re-exported from `nros_core::heapless` for use
in generated code and in user code without a direct `heapless` dependency.

### Sequence fields

| ROS type                       | Rust type                              | Notes                     |
|--------------------------------|----------------------------------------|---------------------------|
| `uint8[]`                      | `heapless::Vec<u8, 64>`                | Capacity is codegen limit |
| `int8[]` / `byte[]`            | `heapless::Vec<u8, 64>`                |                           |
| `bool[]`                       | `heapless::Vec<bool, 64>`              |                           |
| `int32[]` / `float64[]` / etc. | `heapless::Vec<T, 64>`                 |                           |
| `string[]`                     | `heapless::Vec<heapless::String<256>, 64>` |                       |
| `T[]` (nested message)         | `heapless::Vec<T, 64>`                 |                           |
| `T[<=N]` (bounded sequence)    | `heapless::Vec<T, N>`                  | Capacity matches bound    |

The default sequence capacity is **64 elements**. For large-payload
messages where you need more elements at runtime, use the raw
subscription API with `CdrReader` to read fields without materializing
the whole vector.

### Fixed-size arrays

| ROS type | Rust type | Notes                   |
|----------|-----------|-------------------------|
| `T[N]`   | `[T; N]`  | Zero-cost, no heap      |

Arrays with more than 32 elements use a manual `Default` impl instead of
deriving, since `Default` is only derived for arrays up to `[T; 32]`.

### Nested message fields

Nested message types are embedded inline:

| Field type             | Rust type     |
|------------------------|---------------|
| `T` (nested message)   | `T`           |
| `T[]` (sequence of T)  | `heapless::Vec<T, 64>` |
| `T[N]` (array of T)    | `[T; N]`      |

## Traits

Every generated message type implements:

| Trait         | Notes                                              |
|---------------|----------------------------------------------------|
| `Serialize`   | CDR serialization via `CdrWriter`                  |
| `Deserialize` | CDR deserialization via `CdrReader`                |
| `RosMessage`  | Provides `TYPE_NAME` and `TYPE_HASH` constants     |
| `Default`     | All fields zero/empty                              |
| `Clone`       | Deep copy                                          |
| `PartialEq`   | Field-wise equality                                |
| `Debug`       | Formatted output                                   |

`RosMessage` requires `Serialize + Deserialize`:

```rust
pub trait RosMessage: Sized + nros_serdes::Serialize + nros_serdes::Deserialize {
    const TYPE_NAME: &'static str;
    const TYPE_HASH: &'static str;
}
```

## Zero-copy access via raw subscription API

For large messages, deserializing into an owned struct (e.g.,
`Image { data: heapless::Vec<u8, 64> }`) truncates at 64 elements.
Use the raw API and `CdrReader` instead:

```rust
use nros_core::{CdrReader, RosMessage};

// Read only needed fields without full deserialization
executor.add_subscription_buffered_raw::<65536>(
    "/camera/image",
    Image::TYPE_NAME,
    Image::TYPE_HASH,
    QosSettings::default(),
    |cdr: &[u8]| {
        let mut r = CdrReader::new_with_header(cdr).unwrap();
        // Skip header fields by reading and discarding them
        let _stamp_sec: u32 = r.read_u32().unwrap_or(0);
        let _stamp_nsec: u32 = r.read_u32().unwrap_or(0);
        let _frame_id_len: u32 = r.read_u32().unwrap_or(0);
        // ... read only what you need
        let height: u32 = r.read_u32().unwrap_or(0);
        let width: u32 = r.read_u32().unwrap_or(0);
        let pixel_data_len = r.read_u32().unwrap_or(0) as usize;
        let pixels = r.read_slice_u8(pixel_data_len).unwrap_or(&[]);
        process_pixels(width, height, pixels);
    },
)?;
```

The `cdr` slice borrows from the triple buffer slot and is only valid
for the duration of the callback — do not store a reference to it.

## C type mapping

| ROS type                 | C type                                           |
|--------------------------|--------------------------------------------------|
| `string` (unbounded)     | `struct { const char* data; size_t size; }`      |
| `string<=N` (bounded)    | `char name[N]`                                   |
| `uint8[]` (unbounded)    | `struct { const uint8_t* data; size_t size; }`   |
| `T[]` (unbounded, other) | `struct { const T* data; size_t size; }`         |
| `T[<=N]` (bounded)       | `struct { uint32_t size; T data[N]; }`           |
| `T[N]` (fixed)           | `T name[N]`                                      |

The C deserializer sets pointer+length fields to point into the CDR
buffer (valid for the callback duration). The C serializer reads from
the pointer+length fields.

## C++ type mapping

| ROS type                 | C++ type                    |
|--------------------------|-----------------------------|
| `string` (unbounded)     | `nros::StringView`          |
| `string<=N` (bounded)    | `nros::FixedString<N>`      |
| `uint8[]` (unbounded)    | `nros::Span<uint8_t>`       |
| `T[]` (unbounded, other) | `nros::Span<T>`             |
| `T[<=N]` (bounded)       | `nros::FixedSequence<T, N>` |
| `T[N]` (fixed)           | `T name[N]`                 |

`nros::Span<T>` and `nros::StringView` are freestanding C++14 types
defined in `nros/span.hpp`. They provide `data()`, `size()`, `begin()`,
`end()`, and `operator[]` — same API as `std::span` / `std::string_view`.

## Service types

Service request and response types are regular message structs that also
implement `Serialize` and `Deserialize`. Nested message fields in service
types are embedded inline (same as standalone messages).

| Component      | Type                  |
|----------------|-----------------------|
| Request struct | `FooRequest`          |
| Response struct | `FooResponse`        |
| Service marker | `Foo` (zero-sized)    |

`Foo` implements `RosService`:

```rust
impl RosService for Foo {
    type Request = FooRequest;
    type Reply = FooResponse;
    const SERVICE_NAME: &'static str = "...";
    const SERVICE_HASH: &'static str = "...";
}
```

## Action types

Action goal, result, and feedback types are regular message structs.
The action marker type implements `RosAction`:

```rust
impl RosAction for MyAction {
    type Goal = MyActionGoal;
    type Result = MyActionResult;
    type Feedback = MyActionFeedback;
    const ACTION_NAME: &'static str = "...";
    const ACTION_HASH: &'static str = "...";
}
```

`Goal`, `Result`, and `Feedback` all implement `Serialize + Deserialize`
(required by the `RosMessage` bound on `RosAction` associated types).

## Package name remapping

`cargo nano-ros generate --rename old_pkg=new_crate_name` renames:
- Output directory
- `[package] name` in Cargo.toml
- Dependency names and paths in Cargo.toml
- `use old_pkg::` references in Rust source

Used by nano-ros to generate `nros-rcl-interfaces` from `rcl_interfaces`.
