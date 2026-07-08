---
id: 160
title: "QoS struct hand-mirrors drift on every append — no compile-time parity guard (three instances: callback_group, tx_express ×2)"
status: open
type: tech-debt
area: api
related: [issue-0131, issue-0155, issue-0157, issue-0159, phase-273, phase-282]
---

## Summary

`nros_cpp_qos_t` (and the C prototypes around it) exist as **hand-written
mirrors** in more than one header, and every time a field is appended to the
canonical definition a mirror gets missed. Nothing catches the drift at compile
time; the failure surfaces as by-value ABI mismatch (a caller passing a SHORTER
struct than the callee reads → the tail field is stack garbage) or as a
stale-prototype arity mismatch.

Three instances so far:

1. **Phase 273 `callback_group`**: appended to the Rust FFI + C++ header;
   `component.h`'s C prototype for `nros_cpp_sub_raw_register` was missed —
   C callers built against the 9-arg form (documented inline in
   `component.h`).
2. **Phase 282 `tx_express` (init)**: `nros_c_qos_default()` didn't initialize
   the appended field → stack garbage (found in #155/#157, fixed).
3. **Phase 282 `tx_express` (mirror)**: `component.h`'s local `nros_cpp_qos_t`
   mirror (the `#ifndef NROS_CPP_FFI_H` branch, used when a C TU doesn't
   include `nros_cpp_ffi.h` first) was missing the field entirely — mirror-only
   TUs passed a struct one byte short of what `nros_cpp_ffi.h` consumers read
   (found in #159's fresh-rebuild fallout, fixed in `a9f301b37`).

The same class also lives on the ThreadX side: #131's C crash was a stale
config-header mirror.

## Why it keeps happening

The mirror exists so a plain-C TU can use the QoS API without the C++ FFI
header; the canonical struct lives in cbindgen-generated `nros_cpp_ffi.h`
(and `nros_generated.h` for `nros_qos_t`). Appends land in the generator
inputs and regenerate the canonical headers, but the hand mirror in
`component.h` is invisible to that pipeline.

## Direction

Any ONE of these closes the class:

- **Compile-time parity assert** (cheapest): a TU (or header-guarded block)
  that includes BOTH `component.h`'s mirror and `nros_cpp_ffi.h` and
  `static_assert(sizeof(nros_cpp_qos_t) == …)` against a mirrored size
  constant, or `_Static_assert` on `offsetof` of the LAST field. The existing
  `check-c` justfile lane (which already compiles an umbrella-header TU)
  is the natural home.
- **Generate the mirror**: emit `component.h`'s struct block from the same
  source of truth (cbindgen already emits `nros_cpp_ffi.h`; a small post-step
  could splice the struct into `component.h` between markers).
- **Drop the mirror**: make `component.h` `#include` a minimal
  `nros_cpp_qos.h` shared by both headers (no duplicate definition at all).

## References

`packages/core/nros-c/include/nros/component.h` (mirror + phase-273 note),
`packages/core/nros-cpp/include/nros/nros_cpp_ffi.h` (canonical), archived
issues 0131/0155/0157/0159.
