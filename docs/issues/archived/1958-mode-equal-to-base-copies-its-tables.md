---
id: 1958
title: "A contract mode with the base's numbers copies the whole monitor table"
status: resolved
type: improvement
area: [codegen, nros-cpp, monitors]
severity: medium
found: 2026-10-18
related: [1940]
---

## Measured

The safety island's S32K344 image (C++ entry, one executor) grew by 1,840 B
of DTCM when the pin moved past phase-462 W4b/W4c: the generated
`__nros_mode_storage_t0` holds

    (5 + 1) * NROS_CPP_MODE_TABLE_STORAGE + 5 * (6 * NROS_CPP_MODE_ROW_STORAGE
                                                + 1 * NROS_CPP_MODE_AGE_STORAGE)
    = 6 * 40 + 5 * (6 * 48 + 32) = 1,840 B   (32-bit)

for the contract's five modes. Every one of the five baked `__nros_mode_rows`
arrays is identical to the base rows, row for row, and so is every age row:
the modes differ from the base only in their switch budget and reaction (two
of them). `nros_cpp_install_modes` still copied six `MonitorSpec`s and one
`AgeMonitorSpec` per mode. On that image DTCM stood at 119,680 of 131,072 B,
3,200 B above the island's 8 KiB link guard.

## Cause

`lower.rs` bakes each mode's numbers for every row whether they differ or
not, and the install requires `n_rows` / `n_ages` to equal the base's, so a
mode cannot say "the base's numbers".

## Fix

- `nros_cpp_install_modes`: a mode with `rows` NULL and `n_rows` 0 judges by
  the installed base table itself (the slice, no copy, no storage); likewise
  `ages`. Its switch budget, reaction and deadlines apply exactly as before.
  The carve is a separate function (`carve_mode_tables`) so it is unit-tested
  without an executor.
- `lower.rs`: a mode whose rows (ages) equal the base's, row for row, bakes
  none; the C and C++ packs size the storage by the modes with rows (ages) of
  their own.
- The island's five modes then cost their table headers only: 240 B.

## Tests

- nros-cpp `mode_alias_tests` (3): five base-equal modes fit in the headers
  and each aliases the base slices, keeping budget and reaction; a differing
  mode is carved as before beside an aliased one; a short mode is refused.
- nros-node `a_mode_aliasing_the_base_is_entered_and_a_differing_one_still_applies`.
- nros-cli-core `typed_emit_aliases_a_mode_equal_to_the_base` (C and C++);
  the existing `typed_emit_bakes_a_modes_numbers_after_the_monitors` is
  unchanged apart from the storage spelling.
