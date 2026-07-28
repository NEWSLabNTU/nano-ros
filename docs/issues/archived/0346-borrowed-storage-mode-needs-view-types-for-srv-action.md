---
id: 346
title: "`borrowed` storage mode on srv/action payloads needs view types — today it would silently degrade to `owned`"
status: resolved
type: enhancement
severity: low
area: codegen
related: [issue-0343, issue-0344, issue-0345, rfc-0033]
---

## Context

RFC-0033's three storage modes on service/action payloads, after #0343–#0345:

| mode | Rust srv/action | C srv/action | C++ srv/action |
| --- | --- | --- | --- |
| `owned` | yes | yes | yes |
| `heap` | yes (#0344) | yes (#0345) | n/a — FFI-delegated |
| `borrowed` | **rejected — this issue** | **rejected — this issue** | n/a |

`heap` is done in both languages. `borrowed` is the remainder.

## Why it is rejected rather than working

`borrowed` is not a container swap — it emits a **second type** beside the owned
struct:

- Rust: `{Msg}View<'a>` with borrowed fields (`&'a str`, `LeSliceView<'a, T>`)
  plus `impl DeserializeBorrowed`, and a `{Msg}Borrow` ZST marker.
- C: `{Msg}_View` plus
  `int32_t {Msg}_deserialize_borrowed({Msg}_View*, const uint8_t* buf, size_t len)`
  that sets pointers into `buf` (RFC-0033 §borrowed: "No `malloc`, no `_fini`").

The service/action templates emit no view type. Without one, the field builder
keeps the **owned** container for the publish path, so a `borrowed` field would
generate as `owned` — the mode silently does nothing. That is a wrong answer
rather than an error, so `ensure_supported_storage_for_payload()` rejects it and
names the field.

## Worth checking before implementing

**Is a borrowed request even useful here?** For a subscription the borrowed view
aliases the callback's receive buffer, valid for the callback scope. Services and
actions ride the same raw `(data, len)` callbacks
(`nros_service_callback_t` hands `request_data`/`request_len`), so the lifetime
story is identical and a borrowed REQUEST is sound — a handler that only reads
a large array would avoid the copy entirely.

The **response** side is the questionable half: the handler *writes* the response
into a caller-provided buffer, so there is nothing to borrow from. A sensible
scope may therefore be "borrowed requests and action goals/feedback, owned
responses/results", which would need the policy table to become per-payload
rather than per-entity.

## Acceptance

- `{Msg}View` / `{Msg}_View` emitted for whichever payloads are in scope, driven
  from the shared arms added by #0344/#0345 (`_nros_field.jinja`, `_c_field.jinja`)
  rather than new hand-copied clones.
- The two `borrowed`-rejection tests in
  `packages/cli/rosidl-codegen/tests/srv_action_storage_mode_gate.rs` flip to
  asserting generation, and the C side gains a `-fsyntax-only` case in
  `c_heap_compile_check.rs` next to the ones #0345 added.
- RFC-0033's per-entity support matrix updated (it currently discusses messages
  only).

## Why low severity

No in-tree `.srv`/`.action` requests a non-owned mode, and the diagnostic names
the field and mode, so nobody can hit this silently. It is an unfinished corner of
RFC-0033's promise, not a defect.

## Resolution (2026-07-28) — implemented for both payload directions

### The design question this issue raised, answered

The issue asked whether a borrowed RESPONSE is meaningful, since "the handler
writes the response into a caller-provided buffer, so there is nothing to borrow
from". That framing was half wrong. Every payload has **two** sides:

- a **write** side, which keeps the owned struct (RFC-0033 already specifies this
  — the owned container stays for the publish path), and
- a **read** side, which is always a raw buffer.

Both directions read from raw bytes: the server reads `request_data`
(`nros_service_callback_t`), and the client reads `response` — verified,
`nros-c/src/service.rs:898` hands the client callback
`response: *const u8, response_len: usize`. So the view's lifetime story is
identical to a subscription's in both directions, and borrowed is coherent for
every payload. No per-payload policy split was needed.

### What landed

- **View macros** in the shared arm files from #0344/#0345 —
  `view_field_decl` + `borrowed_deser_field` in both `_nros_field.jinja` and
  `_c_field.jinja`. The message templates' own borrowed blocks were re-pointed at
  them first, and byte-identity re-verified, so there is exactly one definition of
  "borrowed field declaration" and "borrowed deserialize arm" per language.
- **Per-payload `has_borrowed_*` flags** on six template structs (service/action ×
  nros/C-header/C-source), computed as `fields.iter().any(|f| f.is_borrowed)`.
- **Emission**: `{Payload}View<'a>` + `impl DeserializeBorrowed` for Rust;
  `{Payload}_View` + `{Payload}_deserialize_borrowed()` for C, with the
  `nros/borrowed.h` include gated on the same flags. The Rust `{Msg}Borrow` ZST
  marker is deliberately NOT emitted for srv/action — it exists to dispatch
  `create_subscription_borrowed`, and there is no equivalent subscription API for
  a service payload; the handler calls `deserialize_borrowed` on the callback
  bytes directly.
- Policy: `(StorageMode::Borrowed, _) => false`. **All three RFC-0033 modes now
  work on service and action payloads in both languages.**

### Verification

- `generated_borrowed_c_service_compiles` — new `gcc -fsyntax-only -Wall -Wextra
  -Werror` case. It caught a real bug: the generated header used
  `nros_borrowed_str_t` without including `nros/borrowed.h`, the same class of
  omission as #0345's missing `nros/platform.h`. Both are now gated includes.
- Owned output byte-identical against the 10-file golden corpus. One near-miss
  worth recording: appending blocks initially added a trailing newline to six
  templates, which changed every generated file's last byte — caught by the same
  diff and stripped.
- 189 default tests green; the `--ignored` lane green at 12 (was 11 before this
  issue's compile check, 2 of which were rotted until #0345 repaired the stubs).

### Final state of RFC-0033 on srv/action payloads

| mode | Rust | C | C++ |
| --- | --- | --- | --- |
| `owned` | yes | yes | yes |
| `heap` | yes (#0344) | yes (#0345) | n/a — FFI-delegated |
| `borrowed` | yes (#0346) | yes (#0346) | n/a — FFI-delegated |

`ensure_supported_storage_for_payload()` now rejects nothing and could be
retired; it is kept as the single place the policy is stated, so a future
emitter that cannot honour a mode has somewhere to say so.
