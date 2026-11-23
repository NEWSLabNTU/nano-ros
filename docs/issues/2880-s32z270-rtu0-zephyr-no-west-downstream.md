---
id: 2880
title: "`s32z270-rtu0-zephyr` has no `west-downstream.yml`, so a consumer hand-pins `hal_nxp`, `cmsis` and `zephyr-lang-rust` to match nano-ros's Zephyr line"
status: open
type: enhancement
severity: low
area: [zephyr, boards, build]
related: [phase-478, 2631, 2771, phase-215]
found: 2026-11-23
---

**Home:** [phase-478](../roadmap/phase-478-island-developer-experience-findings.md)
I40. Consumer: the safety island (autoware-safety-island fork), branch
`nano-ros-next`, nano-ros pin `8d4e70a382`.

## What happens

A board crate can ship a downstream west fragment that a consumer imports with
one line (phase-215 J.3): `fvp-aemv8r-smp` has
`packages/boards/nros-board-zephyr/boards/fvp-aemv8r-smp/west-downstream.yml`
(`zephyr-lang-rust` at nano-ros's pin). `s32z270-rtu0-zephyr` (issue 2631)
has none, and its SoC lives in `hal_nxp`, which nano-ros's own `west.yml`
does not fetch (`nros-sdk-index.toml` `[zephyr_module.hal_nxp]`: "a downstream
one does", by adding it to its OWN manifest). Which `hal_nxp` and `cmsis`
revisions go with nano-ros's Zephyr 3.7 pin is written nowhere a consumer can
import; `gate.yml`'s `zephyr-module-fixtures` job fetches them for itself.

## Island workaround

`actuation_module/west.yml` pins all four by hand, with nano-ros as an
`import: false` leaf:

- `cmsis` and `hal_nxp` "at the revisions Zephyr v3.7.0's own west.yml pins";
  the comment records the break that found them ("SoC s32z270 not supported"
  from an older `hal_nxp`);
- `zephyr` at `v3.7.0` and `zephyr-lang-rust` at nano-ros's supported rev, each with a
  comment to bump it "in lockstep" with nano-ros's `west.yml`.

Nothing checks the four against nano-ros's pins; `nros doctor` (issue 2771)
compares only the nano-ros revision.

## Wanted

`packages/boards/nros-board-zephyr/boards/s32z270-rtu0-zephyr/west-downstream.yml`
declaring `zephyr-lang-rust`, `hal_nxp` and `cmsis` at the revisions nano-ros
tests the board with (the ones `gate.yml`'s module-fixtures job fetches),
kept in lockstep by the check that keeps the FVP fragment's
`zephyr-lang-rust` equal to `west.yml`'s.

## Acceptance

- The fragment exists, and `just check` fails when its `zephyr-lang-rust`
  differs from `west.yml`'s or its `hal_nxp` / `cmsis` from what the
  module-fixtures job fetches.
- The `s32z270-rtu0` module fixture fetches its modules through the fragment.
- The island's `west.yml` imports the fragment and drops its own `cmsis`,
  `hal_nxp` and `zephyr-lang-rust` entries; its `zephyr-s32z` image links.
