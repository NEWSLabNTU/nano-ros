---
id: 238
title: "RMW C-ABI: NrosRmwEventKind is repr(u8) in Rust but the C nros_rmw_event_kind_t is an int-sized enum, passed by value across the vtable"
status: resolved
type: bug
severity: medium
area: rmw
related: [issue-0160, issue-0239]
---

## Finding (RMW/platform API audit, 2026-07-21)

`NrosRmwEventKind` is `#[repr(u8)]`
(`packages/core/nros-rmw-cffi/src/lib.rs:754-755`) and is passed BY VALUE
as a vtable parameter — `register_subscriber_event(sub, kind:
NrosRmwEventKind, …)` / `register_publisher_event(…)`
(`lib.rs:564,572`) — and as the first argument of the event callback
(`rmw_event.h:87` ↔ `lib.rs:804`).

The C counterpart `nros_rmw_event_kind_t`
(`packages/core/nros-rmw-cffi/include/nros/rmw_event.h:36-48`) is an
UNFIXED C enum (no `: int32_t` / `: uint8_t` underlying type), so it is
`int`-sized (4 bytes) by default on every supported toolchain. The Rust
side therefore declares a 1-byte by-value argument where C passes a 4-byte
`int`.

On common SysV/AAPCS ABIs small-int arguments share a register, so this
usually "works" — but it is strictly non-conforming and a latent
portability bug (a target/ABI that passes `u8` and `int` differently, or a
future `-fshort-enums`-style flag, breaks it silently).

## Fix direction

Make the two sides agree on a fixed width. Either:
- give the C enum a fixed underlying type — `typedef enum
  nros_rmw_event_kind_t : int32_t { … }` (C23) or a `_Static_assert` +
  explicit `int32_t` field/param — and change the Rust `repr(u8)` to
  `repr(i32)`; OR
- keep the compact `u8` and make the C signatures take `uint8_t` explicitly
  (the vtable param + callback arg typed `uint8_t`, not the enum).

Same class as the QoS struct's deliberate `uint8_t`-not-`_Bool` choice
(`rmw_entity.h:113-117`) — the event enum just didn't get the same
treatment. A dedicated RMW-mirror drift gate (issue #239) would have caught
this.

## Resolution (2026-07-21)

Rust conforms to C (the C header is the wire contract; C/C++ backends
already treat the kind as `int`). `NrosRmwEventKind` changed
`#[repr(u8)]` → `#[repr(C)]` so it is C-int-sized, matching the unfixed
`nros_rmw_event_kind_t`. Zero header change; no backend edit (they
positionally pass an int-sized enum already). The enum is only ever
passed by value, never stored in a mirrored struct, so the width change
is layout-safe.

**Why `#[repr(C)]` and not a fixed repr:** the C enum is *unfixed*, so
its width follows the target C ABI — `int`-sized on SysV/AAPCS64/RISC-V
but ONE byte on ARM EABI (`-fshort-enums`, the AAPCS32 default; see the
ARM short-enums memory note). `#[repr(C)]` tracks that per-target, so it
matches on both. The old `#[repr(u8)]` happened to match ARM (a 0..=4
enum is 1 byte there anyway) but was the by-value mismatch on host — a
fixed `#[repr(i32)]` would be the mirror-image bug (right on host, wrong
on ARM). So the regression is only observable on int-enum targets.

**Drift now gated on BOTH sides (partial fix for #239, RMW half):**
- Rust: an `abi_layout` const-assert region in `nros-rmw-cffi/src/lib.rs`.
  The event-kind width pin is `#[cfg(not(target_arch = "arm"))]` and
  compares `size_of::<NrosRmwEventKind>() == size_of::<c_int>()` (not a
  hardcoded 4) — gated to the int-enum targets where the bug can occur,
  since on ARM repr(C) and repr(u8) are indistinguishable. A second
  unconditional block pins the QoS size/align, handle-struct pointer
  alignment, and the vtable pointer-slot count. Evaluated at COMPILE
  time — fires on any `cargo build -p nros-rmw-cffi`, host or embedded.
- C: `tests/c_stubs/abi_layout_check.c`, a `_Static_assert`-only TU
  compiled `-fsyntax-only` by the `check-c` push-lane recipe (and under
  the `c-stub-test` feature via build.rs). Compiled host-side (int-enum)
  so it pins `sizeof(nros_rmw_event_kind_t) == 4`, plus the qos size,
  handle alignment, and vtable pointer-slot count.

Both guards verified to fire on injected drift: flipping the Rust repr
back to `u8` fails `assertion failed: size_of::<NrosRmwEventKind>() ==
size_of::<c_int>()` on the host lane. The repr change also surfaced two
latent `core::mem::transmute(kind)` sites in `src/rust_adapter.rs` that
relied on both enums being 1 byte — replaced with an explicit
`From<NrosRmwEventKind> for nros_rmw::EventKind` conversion (a byte
reinterpret is now a size mismatch, i.e. UB, caught by E0512). This is
itself evidence the guard works: the SSoT turned a silent latent UB into
a compile error the moment the width changed.

cbindgen codegen (a true single definition) remains the fuller fix and
is tracked as the forward-looking half of #239; the consistency
assertions here are the bounded, low-risk guard for the width class.
