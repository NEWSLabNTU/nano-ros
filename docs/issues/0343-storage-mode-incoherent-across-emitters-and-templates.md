---
id: 343
title: "RFC-0033 storage modes are incoherent: srv/action templates never branch on heap/borrowed (emitting non-compiling code), and the three emitters disagree on which modes they support"
status: open
type: bug
severity: high
area: codegen
related: [rfc-0033]
---

## Finding (deep audit C,E 2026-07-28 — lead-verified)

### 1. srv/action fields resolve a storage mode the templates ignore

`heap` / `borrowed` **is** resolved for service and action fields —
`generator/srv.rs:306` and `:324`, `generator/action.rs:401-441` all call
`field_to_nros_field_with_mode(f, package_name, …, mode)`.

But only the message template consumes it. Verified by count:

| template | `is_heap` / `is_borrowed` branches |
| --- | --- |
| `templates/message_nros.rs.jinja` | **12** |
| `templates/service_nros.rs.jinja` | **0** |
| `templates/action_nros.rs.jinja` | **0** |

So a `.srv` field configured `mode = "heap"` gets the heap **type** in the struct
(`pub f: nros_core::heap::Vec<T>`, from the resolved field) while the serde body
emits the owned-mode shape (`heapless::Vec::new()` + `.push(..).map_err(..)`) —
**generated code that does not compile**. The failure is silent at config time and
surfaces as a confusing rustc error in generated output.

This is checklist item **D2** ("the two generators stay in sync — guard against
new divergence") landing exactly where it was predicted to.

### 2. The three emitters disagree on the support matrix

For the *same* `nros-codegen.toml` entry:

| emitter | accepts `heap` for |
| --- | --- |
| Rust (`types.rs:717`) | any string, any sequence |
| C (`types.rs:1148`) | strings + primitive/string/bounded-string/nested sequences |
| C++ (`cpp_type_for_field_heap`) | only what it bridges (primitive sequences); **hard-errors on heap strings** |

One config file therefore generates cleanly for Rust and C and fails codegen for
C++ — with no single place that states which (kind, element-kind) → mode
combinations are actually supported.

## Fix

1. Either factor the message template's sequence/string serde arms into a shared
   jinja macro/include that `service_nros.rs.jinja`, `action_nros.rs.jinja`,
   `service_c.c.jinja` and `action_c.c.jinja` all use — or, if srv/action heap
   support is deliberately out of scope, **reject** non-`owned` modes in
   `srv.rs`/`action.rs` with the existing `GeneratorError::UnsupportedStorageMode`
   so the config fails loudly instead of emitting broken code.
2. Put one authoritative support matrix in `config.rs` / RFC-0033 §Storage modes
   (per (kind, element-kind) → supported modes) and validate against it once, in
   `CapacityResolver::resolve`'s caller, so all three emitters accept or reject
   identically. Where a language genuinely cannot honour an entry, the diagnostic
   must name the language.
3. **Add the fixture that holds it shut:** a codegen test that puts a heap field on
   a `.srv` request. Note this interacts with #328 — the 8 `rosidl-codegen`
   compile-check tests that would be the natural home for it are currently
   `#[ignore]`d with no lane running them, so heap/borrowed codegen has *zero*
   executing coverage today.
