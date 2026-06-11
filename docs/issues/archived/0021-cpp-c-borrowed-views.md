---
id: 21
title: Borrowed (zero-copy) message views for C and C++
status: resolved
type: enhancement
area: codegen
related: [rfc-0033, phase-229, phase-235, issue-0007]
resolved_in: Phase 235
---

RFC-0033's `borrowed` storage mode shipped for Rust in Phase 229.6 but C and C++
rejected `mode = "borrowed"`. Phase 235 implemented it for both.

**Resolution (Phase 235).**
- **C** — `nros/borrowed.h` (header-only): `nros_borrowed_str_t` /
  `nros_borrowed_bytes_t` + borrow readers, and alignment-agnostic numeric views
  (`nros_le_slice_view_<t>_t`). Codegen emits `{Msg}_View` +
  `{Msg}_deserialize_borrowed` (pointer-setting, no alloc).
- **C++** — wraps the Rust API (no native C++ CDR reader): a `{Msg}_ffi_
  deserialize_borrowed` walks CDR with the existing reader and fills a
  layout-compatible `{Msg}ViewRepr` (borrowed fields = `nros_cpp_borrow_t
  {*const u8, usize}`); the C++ `{Msg}View` types them as `nros::Span` /
  `StringView` / `LeSpan<T>`. `LeSpan` added to `span.hpp`.
- Numeric sequences borrow via an unaligned LE decoder in both (full Rust parity);
  sequence-of-string / nested rejected.

**Validation.** 86 codegen golden tests + two host runtime E2Es
(`tests/borrowed_{c,cpp}_e2e.sh`): owned-serialize → `deserialize_borrowed` →
assert every view (string / bytes / LE-numeric) aliases the CDR buffer with
correct values. The C++ E2E links the real generated Rust FFI, proving the
repr(C) layout match.

Design + work items: `docs/roadmap/archived/phase-235-c-cpp-borrowed-views.md`. Wire shape
+ rationale: RFC-0033 §"Borrowed mode — C and C++ realization".
