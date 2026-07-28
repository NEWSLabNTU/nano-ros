---
id: 346
title: "`borrowed` storage mode on srv/action payloads needs view types — today it would silently degrade to `owned`"
status: open
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
