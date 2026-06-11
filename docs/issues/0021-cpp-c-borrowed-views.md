---
id: 21
title: Borrowed (zero-copy) message views for C and C++
status: open
type: enhancement
area: codegen
related: [rfc-0033, phase-229, phase-235, issue-0007]
---

**Plan:** the implementation work items live in [Phase
235](../roadmap/phase-235-c-cpp-borrowed-views.md) (C first, then C++). Design
decisions locked there (2026-06): C++ **wraps the Rust API** via an FFI seam that
returns per-field `(offset, len)` (no native C++ CDR reader); numeric sequences borrow
through a `LeSliceView`-equivalent unaligned decoder in C and C++ (full Rust parity,
not rejected).

RFC-0033 `borrowed` storage mode (Phase 229.6) shipped for **Rust** — large
variable-length fields borrow zero-copy from the CDR receive buffer instead of
copying into `heapless`/heap containers. **C and C++ do not yet have borrowed
views**: a `mode = "borrowed"` field generates Rust today and is rejected for
C/C++ (`UnsupportedBorrowedElement` / `UnsupportedStorageMode`).

This is an **optimization, not a functional gap**: C and C++ already represent
large payloads via `mode = "heap"` (rclc-style malloc'd structs / `HeapSequence`,
landed in 229.5). Borrowed adds the **alloc-free** zero-copy path for C/C++ MCUs
that have no heap.

**Target shape (additive, mirroring the Rust `{Msg}View<'a>` + marker):**

- **C** — emit `{Msg}_View` with borrowed fields as
  `struct { const uint8_t* data; size_t size; }` (byte sequences) /
  `struct { const char* data; size_t size; }` (strings), copied fields owned,
  plus `int32_t {Msg}_deserialize_borrowed({Msg}_View*, const uint8_t* buf,
  size_t len)`. The C CDR readers (`nros_cdr_read_u32`, …) already exist; the
  borrowed reader sets pointers into `buf` and bounds-checks against `end`.
- **C++** — emit `{Msg}View` with `nros::Span<uint8_t>` / `nros::StringView`
  fields (`nros/span.hpp` already provides these) + a `deserialize_borrowed`.
  **Blocker:** C++ has no CDR reader today — the owned path deserializes through
  the Rust FFI (`ffi_deserialize`). Borrowed needs either a small pure-C++ CDR
  walk (`nros/cdr_reader.hpp`) or a borrowed FFI returning per-field
  `(offset, len)`.

**Alignment.** Single-byte sequences (`uint8[]`/`int8[]`/`bool[]`) and strings
borrow directly. Multi-byte numerics (`float32[]`, `uint16[]`, …) need the same
alignment-agnostic treatment as Rust's `LeSliceView` — C/C++ `Span<float>` with
`operator[]` does aligned reads and would be UB on an unaligned buffer base, so
either restrict C/C++ borrowed to byte/string fields (reject numerics with a
clear error, as the codegen does now) or add an unaligned-decode span.

**Subscription wiring.** The C/C++ subscription callbacks already receive raw
`(data, len)` (`nros-c` subscription). The borrowed view is a typed accessor
over that same callback — no new ABI, just generated views + the deserialize
helper.

**Where the borrowed codegen is rejected today** (the sites to extend):
`packages/cli/rosidl-codegen/src/generator/common.rs` `build_c_field`
(`StorageMode::Borrowed => Err(unsupported("borrowed"))`) and the C++ field
builder (same pattern).
