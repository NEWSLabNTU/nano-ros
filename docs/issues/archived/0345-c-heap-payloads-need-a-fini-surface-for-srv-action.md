---
id: 345
title: "C heap storage on srv/action payloads needs a `_fini` surface and an ownership convention — the templates emit no fini and no consumer frees"
status: resolved
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

## Resolution (2026-07-28) — and a correction to this issue's premise

**The premise above was wrong, and that is the main finding.** This issue (and
#0344 before it) claimed C heap would require "every C consumer taught to call
`_fini`" — nros-c's request/response paths, the executor, the examples — and
framed it as an ownership-convention change needing an RFC-0033 amendment.

Reading the actual C surface disproved that: `nros_service_callback_t`
(`nros-c/src/service.rs:108`) hands the callback
`request_data: *const u8, request_len: usize` — **raw bytes**. nros-c never
constructs a typed payload struct at all. The *caller* declares it and calls the
generated `_deserialize`, exactly as for messages, so the caller `_fini`s it.
RFC-0033 already says this ("payloads ride the existing raw `(data, len)`
callbacks"); the ownership question I posed did not exist.

So the fix needed no framework change, no consumer change, and no RFC amendment.

### What landed

- **`templates/_c_field.jinja`** — the three mode-dependent C arms (`_fini`
  frees, serialize, deserialize) as shared macros, imported by `message_c.c`,
  `service_c.c` and `action_c.c` (10 call sites). Mirrors the Rust
  `_nros_field.jinja` from #0344.
- **A generated `_fini` per payload struct** — 2 for a service, 3 for an action,
  with header declarations. Emitted unconditionally with an empty body when the
  payload has no heap fields, exactly as `message_c.c` already did.
- **`#include <nros/platform.h>`** in `service_c.c` / `action_c.c`. This was a real
  bug the change introduced and the compile check caught: the fini/deserialize
  arms call `nros_platform_{malloc,free}`, and those templates did not include the
  allocator seam, so the TU failed with implicit declarations.
- Policy: `(StorageMode::Heap, _) => false` — heap is now allowed for both
  languages, `borrowed` still rejected (→ **#0346**).

### Verification

- **`generated_heap_c_service_compiles_and_exposes_fini`** — new
  `gcc -fsyntax-only -Wall -Wextra -Werror` case asserting both payload finis
  exist and the request fini frees the heap sequence. **Passes.**
- Owned output is **purely additive** against a 10-file golden corpus: the only
  changes are the new `_fini` functions/declarations and the new include — zero
  removed or altered lines.
- 189 default tests green; the `--ignored` lane green at 11.

### Bonus: the pre-existing compile check was already broken

`generated_heap_c_message_compiles` — which predates this work — **failed** the
first time it was ever run here. Its stub `cdr.h` was written before phase-303 W4
added the XCDR2 DHEADER seam (`nros_cdr_begin_dheader`, `end_dheader`,
`begin_dheader_read`, `end_dheader_read`, `write_encaps_header`), so every
generated TU had been unbuildable against it since. Nobody noticed because the
test is `#[ignore]`d and **no lane runs `--ignored`** — the exact failure mode
issue **#0328** describes. The stubs are repaired (faithful signatures taken from
`nros-c/include/nros/cdr.h` and `nros_generated.h`), so C heap for MESSAGES is now
verified too, not just asserted.

Practical consequence for #0328: giving the ignored set a lane is now worth
doing. Before this, adding that lane would have turned CI red on a rotted stub.
