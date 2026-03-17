# Codegen Type Mapping Reference

How `cargo nano-ros generate` maps ROS 2 message fields to Rust, C, and C++ types.

## Overview

For messages with all fixed-size fields (e.g., `std_msgs/Int32`), a single
type is generated — identical across borrowed and owned contexts.

For messages with unbounded fields (`string`, `uint8[]`, etc.), **two Rust
types** are generated: a borrowed type (`Msg<'a>`) and an owned type
(`MsgOwned`). C and C++ generate a single struct with pointer+length fields
for unbounded data.

## Rust type mapping

### Primitive fields

| ROS type         | Rust type | Notes |
|------------------|-----------|-------|
| `bool`           | `bool`    |       |
| `byte` / `uint8` | `u8`      |       |
| `char` / `int8`  | `i8`      |       |
| `int16`          | `i16`     |       |
| `uint16`         | `u16`     |       |
| `int32`          | `i32`     |       |
| `uint32`         | `u32`     |       |
| `int64`          | `i64`     |       |
| `uint64`         | `u64`     |       |
| `float32`        | `f32`     |       |
| `float64`        | `f64`     |       |

Primitives are identical in borrowed and owned types. No lifetime.

### String fields

| ROS type               | Borrowed type         | Owned type              |
|------------------------|-----------------------|-------------------------|
| `string` (unbounded)   | `&'a str`             | `heapless::String<256>` |
| `wstring` (unbounded)  | `&'a str`             | `heapless::String<256>` |
| `string<=N` (bounded)  | `heapless::String<N>` | same                    |
| `wstring<=N` (bounded) | `heapless::String<N>` | same                    |

Unbounded strings trigger a lifetime parameter on the struct.
Bounded strings use fixed-capacity `heapless::String` — no lifetime.

### Sequence fields

| ROS type                       | Borrowed type                              | Owned type                  | Zero-copy? |
|--------------------------------|--------------------------------------------|-----------------------------|------------|
| `uint8[]`                      | `&'a [u8]`                                 | `heapless::Vec<u8, 64>`     | Yes        |
| `int8[]` / `byte[]`            | `&'a [u8]`                                 | `heapless::Vec<u8, 64>`     | Yes        |
| `bool[]`                       | `heapless::Vec<bool, 64>`                  | same                        | No         |
| `int32[]` / `float64[]` / etc. | `heapless::Vec<T, 64>`                     | same                        | No         |
| `string[]`                     | `heapless::Vec<heapless::String<256>, 64>` | same                        | No         |
| `T[]` (nested message)         | `heapless::Vec<TOwnedOrT, 64>`             | `heapless::Vec<TOwned, 64>` | No         |
| `T[<=N]` (bounded)             | `heapless::Vec<T, N>`                      | same                        | No         |

Only `uint8[]` and `int8[]`/`byte[]` sequences get true zero-copy (`&'a [u8]`
pointing into the CDR buffer). All other unbounded sequences use
`heapless::Vec` because:

- **Multi-byte primitives** (`int32[]`, `float64[]`): CDR data may not be
  aligned for direct cast to `&[i32]` / `&[f64]`.
- **Bool sequences** (`bool[]`): `bool` and `u8` are different types in Rust.
- **String sequences** (`string[]`): CDR interleaves length prefixes between
  strings — can't return a contiguous `&[&str]`.
- **Nested type sequences** (`Parameter[]`): Elements use Owned variant
  (see below). Can't create `&[Parameter<'a>]` from `&[ParameterOwned]`.

### Nested message fields

| Field type                    | Borrowed struct uses        | Owned struct uses           |
|-------------------------------|-----------------------------|-----------------------------|
| `T` where `T` has no lifetime | `T`                         | `T`                         |
| `T` where `T` has lifetime    | `TOwned`                    | `TOwned`                    |
| `T[]` where `T` has lifetime  | `heapless::Vec<TOwned, 64>` | `heapless::Vec<TOwned, 64>` |

**Nested types with lifetimes always use the Owned variant**, even in the
borrowed struct. This is because `CdrReader`-based deserialization can't
produce borrowed nested types — it would need to slice the buffer at
arbitrary nested field boundaries, which `CdrReader` doesn't support.

The zero-copy benefit comes from top-level fields: `&'a str` for string
fields and `&'a [u8]` for byte sequences. Nested types are small (a few
fields each); the large payloads are always top-level byte sequences
(images, point clouds).

### Fixed-size arrays

| ROS type | Rust type | Notes                      |
|----------|-----------|----------------------------|
| `T[N]`   | `[T; N]`  | Same in borrowed and owned |

Fixed-size arrays never have lifetimes.

## Conversions

For messages with unbounded fields:

```rust
// Borrowed → Owned (explicit copy)
let owned: ImageOwned = msg.to_owned();

// Owned → Borrowed (free borrow)
let borrowed: Image<'_> = owned.as_ref();
```

For nested lifetime types in `to_owned()` / `as_ref()`:
- Direct nested: `self.value.to_owned()` / `self.value.as_ref()`
- Sequence of nested: element-wise iteration with `to_owned()` / `as_ref()`
- Byte sequences: `extend_from_slice()` / `as_slice()`
- Strings: `heapless::String::try_from()` / `.as_str()`

## Trait implementations

| Trait         | Borrowed `Msg<'a>`                | Owned `MsgOwned`                          | Fixed `Msg` |
|---------------|-----------------------------------|-------------------------------------------|-------------|
| `Serialize`   | Yes                               | Yes (delegates to `as_ref().serialize()`) | Yes         |
| `Deserialize` | No (use `deserialize_borrowed()`) | Yes                                       | Yes         |
| `RosMessage`  | Yes                               | Yes                                       | Yes         |
| `Default`     | No                                | Yes                                       | Yes         |
| `Clone`       | No                                | Yes                                       | Yes         |

`RosMessage` has no `Serialize` or `Deserialize` bound — it's a pure
marker trait with `TYPE_NAME` and `TYPE_HASH`.

## C type mapping

| ROS type                 | C type                                         |
|--------------------------|------------------------------------------------|
| `string` (unbounded)     | `struct { const char* data; size_t size; }`    |
| `string<=N` (bounded)    | `char name[N]`                                 |
| `uint8[]` (unbounded)    | `struct { const uint8_t* data; size_t size; }` |
| `T[]` (unbounded, other) | `struct { const T* data; size_t size; }`       |
| `T[<=N]` (bounded)       | `struct { uint32_t size; T data[N]; }`         |
| `T[N]` (fixed)           | `T name[N]`                                    |

C deserializer sets pointer+length fields to point into the CDR buffer
(borrowed, valid for callback duration). C serializer reads from
pointer+length.

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
`end()`, `operator[]` — same API as `std::span` / `std::string_view`.

## Service types

Service request and response types are **always owned**. They use
`MsgOwned` variants for fields that reference message types with
lifetimes.

| Component                 | Type strategy                  |
|---------------------------|--------------------------------|
| Request fields            | Owned (deserialized from CDR)  |
| Response fields           | Owned (constructed by handler) |
| Nested message references | `*Owned` variant               |

Service types implement `Serialize + Deserialize` (required by
`RosService` trait bound).

## Action types

Action goal, result, and feedback types are **always owned** (same
as services). They implement `Serialize + Deserialize` (required by
`RosAction` trait bound).

## Lifetime propagation

A message type gets a lifetime parameter (`Msg<'a>`) if it has:
- An unbounded `string` or `wstring` field (directly)
- A `uint8[]` or `int8[]`/`byte[]` sequence field (directly)
- A direct nested field whose type has a lifetime

Lifetime does NOT propagate through:
- Sequences of nested types (`T[]` where `T<'a>`) — these use `TOwned`
- Bounded sequences or arrays
- Multi-byte primitive sequences

## Package name remapping

`cargo nano-ros generate --rename old_pkg=new_crate_name` renames:
- Output directory
- `[package] name` in Cargo.toml
- Dependency names and paths in Cargo.toml
- `use old_pkg::` references in Rust source

Used by nano-ros to generate `nros-rcl-interfaces` from `rcl_interfaces`.
