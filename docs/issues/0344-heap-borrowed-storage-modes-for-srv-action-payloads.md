---
id: 344
title: "Feature: implement heap/borrowed storage modes for service and action payloads (the srv/action templates have no is_heap branches)"
status: open
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
