---
id: 2880
title: "`s32z270-rtu0-zephyr` has no `west-downstream.yml`, so a consumer hand-pins `hal_nxp`, `cmsis` and `zephyr-lang-rust` to match nano-ros's Zephyr line"
status: resolved
type: enhancement
severity: low
area: [zephyr, boards, build]
related: [phase-478, 2631, 2771, phase-215]
found: 2026-11-23
resolved: 2026-11-24
resolved_in: "feat(zephyr boards, issue 2880): s32z270-rtu0-zephyr ships a checked west-downstream.yml"
---

**Home:** [phase-478](../../roadmap/phase-478-island-developer-experience-findings.md)
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

## Resolved, 2026-11-24

`packages/boards/nros-board-zephyr/boards/s32z270-rtu0-zephyr/west-downstream.yml`
declares the three projects, from the `zephyrproject-rtos` remote:

| project | revision | from |
| --- | --- | --- |
| `zephyr-lang-rust` | `404fcefdbab0f2711cd342a1fab9d2eda52a1ba9` | nano-ros's `west.yml` |
| `hal_nxp` | `862e001504bd6e0a4feade6a718e9f973116849c` | Zephyr v3.7.2's `west.yml` |
| `cmsis` | `4b96cbb174678dcd3ca86e11e1f24bc5f8726da0` | Zephyr v3.7.2's `west.yml` |

`hal_nxp` and `cmsis` are what the module-fixtures job fetches: it builds
against the store workspace at `west.yml`'s Zephyr (v3.7.2), whose own
manifest pins both (`cmsis` through nano-ros's allowlist, `hal_nxp` through
`scripts/zephyr/fetch-job-module.sh`). v3.7.0 pins the same two.

- **The check:** `just check zephyr-west-downstream`
  (`scripts/check-zephyr-west-downstream.py`) walks every tracked
  `west-downstream.yml` (the FVP one too, which had no check: its comment
  only asked for lock-step). It fails when a fragment's `zephyr-lang-rust`
  differs from the board line's manifest, when a Zephyr module in it differs
  from the pin in Zephyr's own `west.yml` at the manifest's Zephyr revision
  (read from the line's provisioned workspace at that commit; a host without
  one prints NOT VERIFIED for the module), when it declares anything else, and
  when a `zephyr_modules` row's board has no fragment declaring the module.
- **The fixture fetches through the fragment:** `fixtures-manifest.py
  compile-check-modules` prints each row's board fragment as a third column,
  and `just zephyr build-module-fixtures` passes it to `fetch-job-module.sh
  --manifest`, which takes the revision and url from the fragment and refuses
  one that differs from the Zephyr tree's pin. So the job builds with what a
  consumer imports, and a Zephyr bump that moves a pin fails the job (which
  runs on any `west.yml` change) until the fragment follows.

**The island** imports the fragment and drops its `cmsis`, `hal_nxp` and
`zephyr-lang-rust` entries; it keeps `zephyr` (at nano-ros's Zephyr revision)
and the nano-ros project. That half waits on its pin.
