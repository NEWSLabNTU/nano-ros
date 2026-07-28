---
id: 345
title: "C heap storage on srv/action payloads needs a `_fini` surface and an ownership convention — the templates emit no fini and no consumer frees"
status: open
type: enhancement
severity: low
area: codegen, core
related: [issue-0343, issue-0344, rfc-0033]
---

## Context

**#0344** implemented RFC-0033 `heap` for **Rust** service/action payloads by
giving every Rust emitter the message template's deserialize arm
(`templates/_nros_field.jinja`). The **C** side stayed rejected, and this issue
records why — it is not an oversight, and it is bigger than a template change.

## Why C is harder than it looks

The C message emitter frees heap fields in a generated function:

```c
void {{ struct_name }}_fini({{ struct_name }}* msg) {
    // for each heap field: free nested elements, free .data, re-zero
}
```

`message_c.c.jinja` emits it (5 `_fini` references). **`service_c.{c,h}.jinja`
and `action_c.{c,h}.jinja` emit none — zero.** So supporting heap there needs:

1. `_fini` functions for each payload struct (2 for a service, 3 for an action),
2. their declarations in the generated headers,
3. **every C consumer taught to call them** — `nros-c`'s service/action request
   and response paths, the executor's reply buffers, and the examples.

Item 3 is the real cost: it is an ownership-convention change on the C API, not
codegen. Today a C service handler receives a request struct by pointer and never
frees anything, because nothing in a service payload allocates. Introducing heap
fields silently changes that contract, and a consumer that misses the call leaks
per request — the worst failure shape for a long-running embedded node.

Generating allocating structs that nobody frees would therefore be worse than the
current diagnostic.

## What would need deciding first

- **Who owns the request?** The executor allocates it before the callback; the
  callback may keep a heap field alive. Either the executor always `_fini`s after
  the callback returns (so the callback must copy anything it keeps), or
  ownership transfers to the callback (so every handler must free).
- **Reply path symmetry** — the response struct is filled by the handler and
  serialized by the executor; whoever allocates should free.
- Whether the C API grows an explicit `nros_service_request_fini()` seam so the
  convention is visible in the header rather than implied by codegen.

That is an RFC-0033 amendment, not a bug fix, which is why this is filed rather
than folded into #0344.

## Current state (from #0344)

| mode | Rust srv/action | C srv/action | C++ srv/action |
| --- | --- | --- | --- |
| `owned` | yes | yes | yes |
| `heap` | **yes** | rejected (this issue) | n/a — FFI-delegated |
| `borrowed` | rejected (no view type) | rejected | n/a |

Enforced by `ensure_supported_storage_for_payload()` in
`generator/common.rs`; covered by `tests/srv_action_storage_mode_gate.rs`.

`borrowed` on srv/action is a separate gap: it works by emitting a
`{Msg}View<'a>` / `{Msg}_View` beside the owned struct, and the srv/action
templates emit no view type, so the mode would silently degrade to `owned`.
Whoever picks that up should check whether a borrowed REQUEST is even meaningful
given the request buffer's lifetime versus the callback scope.
