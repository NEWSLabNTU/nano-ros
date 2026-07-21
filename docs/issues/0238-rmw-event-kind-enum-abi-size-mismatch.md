---
id: 238
title: "RMW C-ABI: NrosRmwEventKind is repr(u8) in Rust but the C nros_rmw_event_kind_t is an int-sized enum, passed by value across the vtable"
status: open
type: bug
severity: medium
area: rmw
related: [issue-0160]
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
