---
id: 21
title: C++ sequence-of-strings FFI repr omits the FixedString size field
status: open
type: bug
area: codegen
related: [phase-229]
---

The C++ codegen renders a `string[]` field's header type as
`nros::FixedSequence<nros::FixedString<N>, cap>`, but the Rust FFI mirror for
the same field uses `[u8; N]` as the element type:

```rust
// generated *_ffi.rs for `string[] tags`
#[repr(C)]
pub struct p_msg_m_t_tags_seq_t {
    pub size: u32,
    pub data: [[u8; 256]; 64],   // element = [u8; 256] = 256 bytes
}
```

```cpp
// generated .hpp for the same field
nros::FixedSequence<nros::FixedString<256>, 64> tags;
```

`nros::FixedString<N>` is `{ uint32_t size; char data[N]; }` (N + 4 bytes), so
the C++ sequence strides its elements by `N + 4`, while the Rust mirror strides
by `N`. The two `#[repr(C)]` views therefore disagree on element boundaries:
the FFI (de)serializer reading the C++ struct will misinterpret every element
after the first.

**Where:** `packages/cli/rosidl-codegen/src/generator/common.rs` —
`build_cpp_ffi_field` sets the sequence-struct `element_type` to
`format!("[u8; {}]", CPP_DEFAULT_STRING_CAPACITY)` for string elements (and the
bounded-string / wstring variants), ignoring the leading `size: u32` of
`FixedString`. The serialize/deserialize template (`message_cpp_ffi.rs.jinja`,
`is_string_element` branches) treats each element as a bare `[u8; N]`.

**Impact:** fixed (non-heap) C++ `string[]` / bounded-`string[]` fields with
more than one element are mis-serialized. Single-element sequences happen to
work because the first element starts at offset 0 in both views.

**Fix direction:** make the Rust mirror element a struct matching
`FixedString<N>` — e.g. `#[repr(C)] struct { size: u32, data: [u8; N] }` — and
have the `is_string_element` serialize/deserialize branches read/write through
the `.data` field (writing `.size` too). A shared
`nros_cpp_fixed_string_t<N>`-style repr (or per-field struct) would centralise
it.

**Blocks:** Phase 229.5 C++ **heap sequence-of-strings** (`mode = "heap"` on a
`string[]`). The heap variant would be `nros::HeapSequence<nros::FixedString<N>>`
and inherits the same element-repr mismatch, so it stays rejected
(`UnsupportedStorageMode`) until this is fixed. Heap `string[]` is already done
on the Rust and C paths (single-level heap of fixed-capacity elements); only the
C++ element repr is blocked.
