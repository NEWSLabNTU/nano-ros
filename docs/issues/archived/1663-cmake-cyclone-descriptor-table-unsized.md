---
id: 1663
title: "On the cmake road Cyclone's `descriptors.cpp` table is always the header's 256 — and the model's type count is the wrong number to raise it from there, because that road registers every LINKED descriptor TU, not the model's types"
status: resolved
resolved_in: 2026-10-05
type: tech-debt
area: [build, cmake, rmw]
severity: low
found: 2026-10-03
related: [1661, 1653, 0280, rfc-0100, phase-454]
---

## What

`NROS_CYCLONEDDS_MAX_DESCRIPTOR_TYPES` sizes `descriptors.cpp`'s static
`Entry g_entries[N]` (16 bytes a row) and `heap_budget.hpp`'s D11 floor. Over
the cap, `register_descriptor` DROPS a registration from a static constructor
that cannot report, and the operator meets it later as `publisher_create`
returning UNSUPPORTED (issue 0280's residual, phase-454 W6.c).

Its single writer is `model_ingest::resolve_cyclonedds_max_descriptor_types`,
which counts the SystemModel's DDS types and only ever RAISES the header's 256.
It is written to the workspace `.cargo/config.toml` `[env]` by `nros
codegen-system`, and `nros-rmw-cyclonedds-sys/build.rs` turns it into a `-D` —
the CARGO road. On the cmake road (`nros_rmw_cyclonedds`, compiled by cmake,
no `-sys` build script) nothing defines it, so it is always 256. Issue 1661
forwarded the other four Cyclone facts and deliberately not this one.

## Why the model count is not the answer there

Measured 2026-10-03 on `examples/workspaces/cpp` `freertos_posix` (cmake,
Cyclone): the descriptor states `[types] distinct_count = 1` (one
`std_msgs/msg/Int32` image), while the linked image holds **36** `*_desc`
descriptor symbols (`nm ... | grep -c '_desc$'`) out of 81 generated register
TUs — `nros_generate_interfaces` emits a descriptor for every message of every
found package, and each linked register TU's constructor registers one. So on
this road the table must hold the LINKED descriptor count, which the model does
not know: sizing it from the model would UNDER-size it (1 against 36), which is
exactly the silent drop the knob exists to prevent. Today 256 covers it with
room; an image linking more than 256 descriptor TUs drops registrations on the
cmake road while the same image on the cargo road would raise the cap.

## Direction

The count belongs to whoever knows which register TUs are compiled into
`nros_rmw_cyclonedds`'s consumers — the cmake side that generates them
(`nros_rmw_cyclonedds_generate_from_msg`). An upper bound (every generated TU,
81 here) is safe; an exact linked count needs the link. Either way it is a
cmake-side single writer for the cmake road, not a second call of the model
resolver. Until then the header's 256 is the cap and the D11 floor reads it.

## Acceptance

A cmake Cyclone image that generates more than 256 descriptor TUs compiles its
table at least that large, and a test asserts the definition reaches
`nros_rmw_cyclonedds`.

## Resolution, 2026-10-05

The issue's direction: the cmake side that GENERATES the register TUs is the
single writer for the cmake roads.

* `nros_rmw_cyclonedds_idlc_compile` — the one place every register TU is
  written — records each registry key it generates one for
  (`nros_cyclonedds_record_descriptor_types`, in the new
  `packages/rmw/cyclonedds/nros-rmw-cyclonedds/cmake/NrosRmwCycloneddsDescriptorCap.cmake`).
* One deferred call at the end of the top-level directory counts the DISTINCT
  keys (`register_descriptor` dedupes by name), and when that exceeds the
  header's 256 puts `NROS_CYCLONEDDS_MAX_DESCRIPTOR_TYPES=<n>` on the target
  that compiles `descriptors.cpp` — `nros_rmw_cyclonedds`, or the Zephyr
  module's `nros` — so `heap_budget.hpp`'s D11 floor reads the same value.
  It only ever RAISES, as the cargo road's writer does, so every image that
  fits compiles byte-for-byte as before. A pin (`-D` or the environment
  variable) wins when it covers the demand and is refused when it does not.
  An IMPORTED backend compiles nothing and the skip is reported.
* An upper bound, by design: a generated register TU that is never linked
  registers nothing, and over-sizing costs 16 B a row while under-sizing drops
  registrations silently.

**Measured** on `examples/workspaces/cpp` `freertos_posix` (cmake, Cyclone):
the configure reports `keeps its default 256 rows (81 distinct type(s)
generated)` — the issue's 81 register TUs — and `g_entries` is 0x1000 (256 ×
16), unchanged. With the cap pinned to 300 the same image compiles
`g_entries` at 0x12c0 (300 × 16), so the definition reaches the compiler of
`nros_rmw_cyclonedds`; pinned to 64 the configure is refused ("pinned to 64,
but this configure generates registrations for 81").

Test: `just check cyclonedds-descriptor-cap` (`tests/cmake-cyclonedds-descriptor-cap-tests.sh`),
five configure-and-build cases whose stand-in TU `static_assert`s the value
it was compiled with, recorded from a SUBDIRECTORY as a real configure does.
Cases A and C fail with the `target_compile_definitions` line removed; the
first cut's scoping bug (the default read from a variable the top-level
deferred call cannot see) is what case B now asserts against.
