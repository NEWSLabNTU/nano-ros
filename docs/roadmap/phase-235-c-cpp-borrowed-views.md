# Phase 235 — Borrowed (zero-copy) message views for C and C++

**Goal.** Bring RFC-0033's `borrowed` storage mode to **C and C++** codegen, mirroring
the Rust path shipped in Phase 229.6. A `mode = "borrowed"` field on a large
variable-length payload becomes an alloc-free, callback-scoped view that points into
the live CDR receive buffer instead of copying into a fixed array / heap container.

**Status.** Planned (2026-06). Implements [issue
0021](../issues/0021-cpp-c-borrowed-views.md); continues Phase 229.6 (Rust borrowed).

**Priority.** P2 — optimization, not a functional gap. C/C++ already carry large
payloads via `mode = "heap"` (229.5); borrowed adds the heap-free path for MCUs with
no allocator. Issue 0007 is already closed (via Rust borrowed + C/C++ heap).

**Depends on.** RFC-0033 (storage-mode design + the `{const T* ptr; size_t len;}`
C/C++ borrowed shape, RFC-0033 §"Borrowed mode" + the storage table); Phase 229.6
(Rust `{Msg}View<'a>` + `DeserializeBorrowed` + `create_subscription_borrowed`, the
reference implementation); the existing C CDR readers (`nros-c/include/nros/cdr.h`)
and C++ view types (`nros-cpp/include/nros/span.hpp` — `nros::Span<T>`,
`nros::StringView`).

## Design decisions (resolved 2026-06)

1. **C++ wraps the Rust API — no native C++ CDR reader.** C++ never re-implements CDR
   parsing. The borrowed C++ view is built from per-field `(offset, len)` computed by a
   **Rust FFI** walk (extending the existing `ffi_deserialize` seam), then the C++
   `{Msg}View` sets `nros::Span` / `nros::StringView` into the raw callback buffer.
   (Rejected: a pure-C++ `cdr_reader.hpp`.)
2. **Numeric sequences borrow via unaligned decoders.** `float32[]`, `uint16[]`, … are
   *not* rejected: C and C++ get a `LeSliceView`-equivalent (byte-by-byte `memcpy` +
   little-endian decode per element, no alignment assumption), matching Rust's
   `nros_core::LeSliceView`. Byte sequences (`uint8[]`/`int8[]`/`bool[]`) and strings
   borrow directly as `{const T* ptr; size_t len;}`. Full parity with Rust borrowed.
3. **C first, C++ after.** C borrowed is unblocked (readers + view structs exist) and
   ships as Increment 1; C++ borrowed (the FFI-offset seam + wrapper) follows as
   Increment 2.

## Architecture

```
                        owned/heap (today)            borrowed (this phase)
 C    subscription cb ──► {Msg}_deserialize ─────►    {Msg}_deserialize_borrowed
      (data,len)          (copies into arrays/        (sets {const T* ptr; size_t len}
                           malloc)                     into `data`; pure-C, no alloc)

 C++  subscription cb ──► ffi_deserialize ──────►     ffi_deserialize_borrowed (Rust)
      (data,len)          (Rust fills repr(C))         → per-field (offset,len)
                                                       → {Msg}View sets Span/StringView
                                                         into `data` (C++ wraps Rust)
```

No subscription ABI change: both C (`nros_subscription_callback_t`) and C++
(`nros_cpp_subscription_message_callback_t`) callbacks already receive raw `(data,
len)`. The borrowed view is a typed accessor over that same buffer.

## Reference (what to mirror) — Rust borrowed, Phase 229.6

- `generator/common.rs::nros_borrowed_view_for_field` (~66) maps a field →
  (`&'a [u8]` / `&'a str` / `nros_core::LeSliceView<'a, T>`, reader expr).
- `templates.rs` (~163-202) carries `borrowed_rust_type` / `borrowed_read_expr` on
  `NrosField`; `templates/message_nros.rs.jinja` (~187-303) emits `{Msg}View<'a>` +
  `{Msg}Borrow` marker + `impl DeserializeBorrowed`.
- `nros-serdes/src/cdr.rs::LeSliceView` (~565-613) — the unaligned numeric decoder to
  port to C/C++.

## Work Items

### Increment 1 — C borrowed (unblocked)

#### 235.1 — C borrowed field mapping  ⬜
Add `c_borrowed_view_for_field` (parallel to `nros_borrowed_view_for_field`) and
extend `CField` (`generator/common.rs:347-507`) with `is_borrowed`,
`borrowed_c_type`, `borrowed_read_expr`. Replace `StorageMode::Borrowed =>
Err(unsupported("borrowed"))` (`common.rs:393`) with the mapping:
- string → `struct { const char* data; size_t size; }`
- `uint8[]`/`int8[]`/`bool[]` → `struct { const uint8_t* data; size_t size; }`
- `uint16[]`/`float32[]`/… → the 235.2 unaligned-view struct.
Sequence-of-string / sequence-of-nested stay rejected (`UnsupportedBorrowedElement`,
`common.rs:44,72`) — same as Rust.
- **Files:** `packages/cli/rosidl-codegen/src/generator/common.rs`, `templates.rs`.

#### 235.2 — C unaligned numeric decoder  ⬜
Add a `LeSliceView` equivalent to `nros-c/include/nros/cdr.h`: per-numeric-type
`{ const uint8_t* bytes; size_t count; }` view + `..._get(view, i)` that `memcpy`s
`sizeof(T)` bytes and little-endian-decodes (no alignment assumption). Generated for
the element types a borrowed numeric field can use (`u16/i16/u32/i32/u64/i64/f32/f64`).
- **Files:** `packages/core/nros-c/include/nros/cdr.h` (+ `cdr.c` if non-inline).

#### 235.3 — C borrowed view type + deserialize  ⬜
Emit `{Msg}_View` (borrowed fields as the 235.1 structs, copied fields owned) and
`int32_t {Msg}_deserialize_borrowed({Msg}_View* out, const uint8_t* buf, size_t len)`
that walks CDR, bounds-checks against `end`, and **sets pointers into `buf`** for
borrowed fields (copies owned fields with today's logic). No `malloc`, no `_fini`.
- **Files:** `templates/message_c.h.jinja`, `templates/message_c.c.jinja`.

#### 235.4 — C tests + example  ⬜
Golden codegen tests (byte-seq, string, numeric-seq borrow; seq-of-string → error)
in `generator/mod.rs`; an E2E that owned-publishes → borrowed-subscribes in C and
asserts the view pointers alias the callback buffer (no copy, no alloc).
- **Files:** `generator/mod.rs` golden tests; a C example/fixture under
  `examples/*/c/` or `packages/testing/`.

### Increment 2 — C++ borrowed (FFI-offset seam)

#### 235.5 — Rust FFI borrowed-offsets seam  ⬜
Extend the C++ FFI (`templates/message_cpp_ffi.rs.jinja`, `build_cpp_ffi_field`
~`common.rs:603`) with `{Msg}_ffi_deserialize_borrowed` that walks CDR with the
existing Rust reader and writes a per-borrowed-field `(offset, len)` struct (offsets
relative to `buf`), instead of copying into the repr(C) struct.
- **Files:** `templates/message_cpp_ffi.rs.jinja`, `generator/common.rs`.

#### 235.6 — C++ unaligned Span decoder  ⬜
Add an alignment-agnostic numeric view to `nros-cpp/include/nros/span.hpp` (e.g.
`nros::LeSpan<T>` with `T operator[](i)` doing `memcpy`+LE-decode), mirroring
235.2 / Rust `LeSliceView`. `Span<uint8_t>` / `StringView` already exist for the
byte/string cases.
- **Files:** `packages/core/nros-cpp/include/nros/span.hpp`.

#### 235.7 — C++ borrowed view type + wrapper  ⬜
Replace `StorageMode::Borrowed => Err(unsupported("borrowed"))`
(`resolve_cap_override`, `common.rs:554`) with the borrowed mapping. Emit `{Msg}View`
(borrowed fields `nros::Span<uint8_t>` / `nros::StringView` / `nros::LeSpan<T>`,
copied fields owned) + a `deserialize_borrowed(view, buf, len)` that calls the 235.5
FFI to get offsets, then sets the spans into `buf` (C++ wraps Rust).
- **Files:** `generator/common.rs`, `templates/message_cpp.hpp.jinja`.

#### 235.8 — C++ tests + example  ⬜
Golden tests + an E2E mirroring 235.4 in C++ (spans alias the callback buffer).
- **Files:** `generator/mod.rs`; a C++ example/fixture.

### Close-out

#### 235.9 — Resolve issue 0021 + sync docs  ⬜
Mark issue 0021 `resolved` (move to `archived/`), tick RFC-0033's storage table /
phase-229 229.6 cross-ref to note C/C++ borrowed landed.
- **Files:** `docs/issues/0021-*`, `docs/issues/README.md`, `docs/design/0033-*`,
  `docs/roadmap/phase-229-*`.

## Acceptance

- `mode = "borrowed"` generates compiling C and C++ for byte-seq, string, and numeric
  (unaligned-decoded) fields; sequence-of-string/nested rejected with a clear error.
- Borrowed C `{Msg}_deserialize_borrowed` and C++ `{Msg}View` set pointers/spans into
  the callback buffer with **zero allocation** (assert pointer aliasing in E2E).
- Numeric borrowed reads are correct on an unaligned buffer base (port of the Rust
  `LeSliceView` odd-offset test).
- C++ borrowed adds **no** native CDR parsing — it is a wrapper over the Rust
  FFI-offset seam.
- `just ci` green; issue 0021 → resolved/archived.

## Notes

- RFC-0033 already specifies the C/C++ borrowed wire shape (`{const T* ptr; size_t
  len;}`) and the hard constraints (callback-scoped, read-only, receive-only) — this
  phase is implementation only, no RFC change.
- Borrowed subscriptions are triple-buffer / depth-1 only on the Rust runtime side
  (229.6); the C/C++ views ride the same `(data, len)` callback, so they inherit that
  constraint without new runtime work.
