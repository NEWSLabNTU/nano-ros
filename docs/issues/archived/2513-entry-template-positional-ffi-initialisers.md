---
id: 2513
title: "The C and C++ entry templates initialise cbindgen FFI structs by position, so a field inserted mid-struct mis-assigns every generated entry with no gate noticing"
status: resolved
resolved: 2026-11-19
resolved_in: "the change that archives this issue"
type: tech-debt
severity: medium
area: [codegen, c-api, cpp-api]
found: 2026-11-14
related: [0160]
---

## What

The entry codegen writes the monitor and hazard tables that an image hands to
`nros-cpp` as static arrays of cbindgen-declared structs
(`packages/api/nros-cpp/include/rclcpp/detail/nros_cpp_ffi.h`). Both templates
initialise them by POSITION:

- `packages/cli/nros-cli-core/src/codegen/entry/packs/entry/c/entry.c.jinja`
  (12 `static const nros_cpp_*_t` tables: monitor rows, age rows, hazard
  members, monitors, rungs, rows, ...);
- `packages/cli/nros-cli-core/src/codegen/entry/packs/entry/cpp/monitor_statics.jinja`
  (the same tables for a C++ entry).

For example, a hazard rung is written as
`{ mode, output_row, route_ms, role, starts_at, starts_at_len, "level" }`.
The Rust side declares the struct (`#[repr(C)]` in `packages/api/nros-cpp/src/lib.rs`),
cbindgen regenerates the header from it, and the template has to list the
fields in the same order. Nothing checks that it does.

An APPEND is safe: an older initialiser leaves the new tail zeroed, and phase-560
W8's `level` (PR replacing #3763) was appended for that reason. An INSERT, or
a reorder, is not: every later field takes its neighbour's value. When the
types are compatible (most of these fields are `uint32_t`) it compiles clean
and the image runs with wrong budgets, rows or roles.

## Why no gate catches it

`check-ffi-struct-mirrors` covers two families: hand-written header mirrors of
`nros_cpp_ffi.h` structs, and the tier table's codegen initialisers, which it
can check because those use DESIGNATED initialisers and it reads the `.field`
names back. The monitor/hazard tables have no hand mirror (so family 1 has
nothing to compare) and positional initialisers (so family 2 has no names to
read). The cbindgen header's own staleness gate proves the header matches the
Rust struct, which is the half that was never at risk.

## Fix direction

- **C template:** designated initialisers (C99), then add these tables to
  family 2 of `check-ffi-struct-mirrors`.
- **C++ template:** designated initialisers are C++20, the cross toolchains
  build `-std=c++14`, and `cmake/toolchain/NanoRosCrossToolchain.cmake` records
  arm-none-eabi-g++ 10.3.1 refusing C99 designators in C++ at every standard.
  So the C++ side needs a different check: emit
  `static_assert(offsetof(T, field) == N)` per field in declaration order, or
  generate the template's field order from the same struct description
  cbindgen reads, so the order is one fact rather than two.

Acceptance: a mutation that swaps two `uint32_t` fields of
`nros_cpp_hazard_rung_t` in the Rust struct fails a gate (or the build) before
any image is produced.

## Resolved (2026-11-19)

- **C pack:** every FFI row is a designated initialiser. That covers the 12
  monitor, hazard and mode tables, `node_body`'s QoS overrides and
  `boot_wrapper`'s executor backings. An insert or reorder in the struct
  cannot shift a value.
- **C++ pack:** C++14 cannot use designators, so `cpp/_ffi_layout.jinja` states
  the positional order of each struct it writes once. Every C++ entry (and
  the PX4 entry) carries a `static_assert` chain over that order: the first
  field at offset 0, each next field exactly at the aligned end of the one
  before, and `sizeof` exactly at the aligned end of the last. 67 asserts in
  a native entry; they compile clean against the real headers.
- **Fast line:** `check-ffi-struct-mirrors` family 5 compares each
  `_ffi_layout.jinja` list with the cbindgen header's declaration order, and
  refuses a C-pack row that is positional or names a field the header lacks.

Measured against mutations of a copy of `nros_cpp_ffi.h`:

| mutation | caught by |
| --- | --- |
| swap `mode` / `output_row` in `nros_cpp_hazard_rung_t` | the build: "the entry writes mode first" |
| insert `uint32_t` after `ftti_ms` in `nros_cpp_hazard_row_t` | the build: "the entry writes settle_ms right after ftti_ms" |
| a C row back to positional | family 5: "POSITIONAL row" |
| a reordered `_ffi_layout.jinja` list | family 5: "entry layout DRIFTED" |

Not covered: the `NROS_SCHED_TASK_MEMORY` macros, which are hand-written
in the same header as the struct they fill and shared with C++.
