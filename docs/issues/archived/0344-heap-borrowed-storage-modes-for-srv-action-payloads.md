---
id: 344
title: "Feature: implement heap/borrowed storage modes for service and action payloads (the srv/action templates have no is_heap branches)"
status: resolved
type: enhancement
severity: low
area: codegen
related: [issue-0343, rfc-0033]
---

## Context

Issue **#343** closed the correctness hole: a non-`owned` RFC-0033 storage mode on
a service or action payload used to emit the heap TYPE with an owned-mode serde
body (generated code that could not compile), and now fails loudly at config time
with `UnsupportedStorageModeForPayload`.

This issue tracks actually **supporting** those modes, which is a feature, not a
bug fix.

## What is missing

`is_heap` / `is_borrowed` branch counts per template:

| template | `is_heap` | `is_borrowed` |
| --- | --- | --- |
| `message_nros.rs.jinja` | 12 | present |
| `message_c.c.jinja` | 6 | 1 |
| `service_nros.rs.jinja` | 0 | 0 |
| `service_c.{c,h}.jinja` | 0 | 0 |
| `action_nros.rs.jinja` | 0 | 0 |
| `action_c.{c,h}.jinja` | 0 | 0 |

The Rust and C **field builders** already resolve and honour the modes
(`field_to_nros_field_with_mode`, `build_c_field`), so the missing piece is purely
the serde/fini bodies in the six srv/action templates.

C++ needs nothing: its templates delegate serialization across the FFI to the Rust
core (nano-ros C++ codegen wraps Rust and never reimplements CDR), so the
container type is the only thing that changes.

## Suggested approach

Factor the message templates' sequence/string serde arms into shared jinja
macros/includes and have the six srv/action templates use them. That is the fix
the checklist's **D2** item ("the two generators stay in sync — guard against new
divergence") points at: today the duplication is what allowed the divergence, so
adding six more hand-written copies would be the wrong shape.

Acceptance: extend `tests/srv_action_storage_mode_gate.rs` — the cases that
currently assert an ERROR flip to asserting successful generation, plus a
compile-check in the `*_heap_compile_check.rs` family (note #328: those suites are
`#[ignore]`d with no lane running them, so that half needs a lane first).

## Why low severity

Nothing needs this yet — no in-tree `.srv`/`.action` uses a non-owned mode, and
the diagnostic from #343 tells anyone who tries exactly what to do. It is a real
capability gap in RFC-0033's promise, not a defect.

## Resolution (2026-07-28) — Rust implemented, C and `borrowed` deferred with reasons

### What the templates actually needed

Less than the issue assumed. Rust **serialization is already storage-mode
agnostic** — `.as_str()`, `.len()` and `&self.field` iteration work for
`heapless` and `heap` alike, which is why `message_nros.rs.jinja` had no
`is_heap` branches in its serialize block. The struct field type comes from the
field builder, which srv/action already call. So the entire divergence was the
**deserialize arm**: container construction plus `push` error handling.

That made the shared surface exactly one macro.

### What landed

- **`templates/_nros_field.jinja`** — the one deserialize arm, with the heap
  branches, imported by `message_nros.rs.jinja`, `service_nros.rs.jinja` and
  `action_nros.rs.jinja` (6 call sites: 2 message incl. the borrowed view, 2
  service, 3 action). askama 0.12 supports `{% import %}` / `{% macro %}` /
  `{% call %}`; there was no precedent in this repo, so it was spiked first.
- **Proven output-preserving.** A 10-file golden corpus (msg/srv/action × C/Rust
  + the inline emitter) was captured before the refactor and re-diffed after.
  Result: byte-identical except one intended convergence — the srv/action
  string-sequence arm used a nested
  `vec.push(String::try_from(s)?)` where the message arm uses
  `let elem = …; vec.push(elem)`. Semantically identical, and **no committed
  generated file carries the old shape**, so nothing in-tree regenerates
  differently.
- **`ensure_supported_storage_for_payload()`** replaces #0343's blanket
  rejection with a per-language policy table (`PayloadLang::{Rust, C}`), so the
  guard now states exactly what is supported where.
- **8 tests** in `tests/srv_action_storage_mode_gate.rs`, including the defect
  inverted into a regression gate: the struct field type and the deserialize body
  must AGREE (heap type ⇒ heap container ⇒ infallible push). That is the assertion
  whose absence let #0343 exist.

Verified: heap on a Rust service now emits
`pub values: nros_core::heap::Vec<i64>` with
`let mut vec = nros_core::heap::Vec::new();` and `vec.push(reader.read_i64()?);`
— before, the same config produced the heap type with `heapless::Vec::new()` and
a `.map_err(CapacityExceeded)` push, which does not compile.

### What did NOT land, and why

- **C heap** — the C message emitter frees heap fields in a generated
  `{Struct}_fini()`; `service_c.*` and `action_c.*` emit **no `_fini` at all**.
  Supporting heap there needs the fini functions, header declarations, and every
  C consumer (nros-c's request/response paths, the executor, examples) taught to
  call them. That is an ownership-convention change on the C API, and generating
  allocating structs nobody frees would leak per request. Filed as **#0345**.
- **`borrowed`** — works by emitting a `{Msg}View<'a>` beside the owned struct;
  srv/action emit no view type, so the mode would silently degrade to `owned` —
  a wrong answer rather than an error. Also #0345.

Both remain hard errors naming the field, mode and entity.

### Verification

`rosidl-codegen` 189 tests green (8 in the new suite); golden-corpus diff as
above; `cargo +nightly fmt --all` clean; `just setup-cli` + `just check fast`
green.
