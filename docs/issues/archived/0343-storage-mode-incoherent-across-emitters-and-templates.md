---
id: 343
title: "RFC-0033 storage modes are incoherent: srv/action templates never branch on heap/borrowed (emitting non-compiling code), and the three emitters disagree on which modes they support"
status: resolved
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

## Resolution (2026-07-28)

The defect was **wider than filed**: the audit named the Rust srv/action
templates, but `message_c.c.jinja` has 6 `is_heap` branches while
`service_c.*`/`action_c.*` have zero — so the **C** emitter had the identical
hole, and `build_c_field` happily accepts heap for bridgeable shapes. C++ turned
out NOT to be affected: its templates delegate serialization across the FFI to the
Rust core (nano-ros C++ codegen wraps Rust, never reimplements CDR), so the
container type is the only thing that changes there.

### What landed

- **`ensure_owned_storage_for_payload()`** (`generator/common.rs`) — rejects a
  non-`owned` mode on any service/action payload struct, with a diagnostic naming
  the entity kind, package, message, field and mode. Wired into **all six** entry
  points: `generate_nros_service_package`, `generate_nros_inline_service`,
  `generate_c_service_package`, and the three action equivalents. This is the same
  behaviour the C field builder already had for shapes it cannot bridge — the fix
  makes the emitters consistent rather than inventing a new policy.
- **New error variant** `UnsupportedStorageModeForPayload`. The existing
  `UnsupportedStorageMode` text ("Phase 229 ships 'owned'…") is now false for
  messages, so reusing it would have propagated the stale claim.
- **`StorageMode::is_phase1_supported()` deleted.** It had **no production
  callers** — only its own unit test asserted on it — and its claim ("only
  `owned`") had become false, since messages implement heap and borrowed. A
  predicate nobody calls, asserting something untrue, reads as a gate while
  gating nothing. Replaced with the real per-language/per-entity support matrix
  as a doc comment naming the code that enforces each cell. (This also closes the
  deep audit's P3 about a phase number baked into a public API name.)
- **`tests/srv_action_storage_mode_gate.rs`** — 7 tests. They assert the
  DIAGNOSTIC, so unlike the `*_heap_compile_check` suites they need no toolchain
  and run in the default lane. Includes the regression half: heap on a MESSAGE
  must still generate, and `owned` services must be untouched — a blanket
  rejection would have been the easy wrong answer.

### Verification

`rosidl-codegen`: 189 tests green across 14 binaries (7 new). Whole
`packages/cli` workspace builds and tests clean. `cargo +nightly fmt --all` clean,
`just check fast` green, `just setup-cli` rebuilt.

### Deferred

Actually **supporting** heap/borrowed on srv/action payloads is a feature, filed
as **#344** — the fix there is to factor the message templates' serde arms into
shared jinja macros rather than hand-writing six more copies (the duplication is
what allowed this divergence in the first place; checklist item D2).

### Note on the second half of this issue

The three emitters genuinely do accept different (kind, element-kind) shapes for
`heap` — Rust the most, then C, then C++ (primitive sequences only). That is now
**documented in one place** (the `StorageMode` matrix comment) with each cell
naming its enforcing function, rather than silently diverging. Making the three
accept identical sets is part of #344's scope, not a separate defect: each
emitter already rejects what it cannot bridge, so no configuration silently
produces wrong output today.
