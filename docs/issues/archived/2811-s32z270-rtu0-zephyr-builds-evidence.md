---
id: 2811
title: "The S32Z270 RTU0 Zephyr board had no build evidence: no fixture built it, and a Zephyr fixture's west board id counted for no row"
status: resolved
type: enhancement
severity: low
area: [zephyr, build, ci, boards]
related: [phase-524, 2631, 2632]
found: 2026-11-21
resolved: 2026-11-21
resolved_in: "phase-524 W3"
---

**Home: [phase-524](../../roadmap/phase-524-emulator-twins.md) W3** (board
descriptors). Follows [#2631](2631-s32z270-rtu0-zephyr-board-descriptor.md),
which added the `s32z270-rtu0-zephyr` descriptor and linked it once by hand.

## What was missing

The board row `nros-board-zephyr@s32z270-rtu0-zephyr` read "Not supported":

- no fixture built the board. Its SoC's devicetree and drivers are in
  `hal_nxp`, which the west allowlist leaves out (`[zephyr_module.hal_nxp]
  lines = []`, 1.3 GB), so no lane could;
- the build-evidence derivation could not see a Zephyr fixture anyway. A row
  counted as built only with a Runtime/BuildOnly cell on its matrix platform or
  a fixture `NROS_BOARD = "<key>"`. A west fixture names its board as `west
  build -b` does (`board =` / `west_board =`) and reads no `NROS_BOARD`.
- the two copies of that derivation disagreed. `check-board-tiers.py`'s
  `row_built` ignored a shared package's board key, so `--print-status` read
  the S32Z270 RTD variant as "Builds" from its FreeRTOS sibling's
  `NROS_BOARD`, while the generated views said "Not supported".

Found on the way: `nros build` looked for an image's per-board fragments in
`boards/s32z2xxdc2@D_s32z270_rtu0/`. `sanitize_board` kept the revision that
#2631 had dropped from the per-board file stem.

## Fix

- **Derivation.** `board_registry.row_built` is the one derivation both
  scripts call. On top of the two existing sources it counts a Zephyr fixture
  that builds an image (`[[fixture]]` / `[[workspace_fixture]]` `board`, a
  `west-build` compile-check row's `west_board`) whose west board id is the
  row's descriptor's `[board.zephyr] west_board`. It is keyed per descriptor
  because `nros-board-zephyr` carries three Zephyr boards. Putting
  `NROS_BOARD` on a west row was the other option. It would be a key whose
  only reader is the evidence regex, since a west build never reads it.
  Today only this row gains from the change. The FVP and Pico W have no
  fixture, and the boards that do already have Runtime cells. Negative
  controls are in `board_registry.self_test`.
- **Fixture.** `[image.s32z270_rtu0]` in `examples/workspaces/cpp` (the C++
  Cyclone talker + listener) on `s32z270-rtu0-zephyr`, with its own
  `boards/s32z2xxdc2_s32z270_rtu0/prj.conf` (C++ runtime, `MAIN_STACK_SIZE`)
  and `prj-cyclonedds.conf`. The compile-check row `west_s32z270_rtu0_zephyr`
  (`west-build`, `west_nros_image`, `zephyr_modules = ["hal_nxp"]`) builds it.
- **Job modules.** `zephyr_modules` names modules that the index declares and
  the allowlist leaves out. A run supplies them as `<name>=<dir>` in
  `NROS_ZEPHYR_JOB_MODULES`. Until a run does, the row is in no listing (build,
  staleness probe, lane dirs), so the default Zephyr lane neither builds it nor
  expects it. `west-fixtures.sh` passes the modules as `EXTRA_ZEPHYR_MODULES`
  with this checkout's nros module first, because Zephyr reads that variable
  ahead of `ZEPHYR_EXTRA_MODULES` and stops at the first one it finds.
  `scripts/zephyr/fetch-job-module.sh` fetches a module at the revision the
  Zephyr tree pins: shallow, blobless and sparse, so `hal_nxp` is ~150 MB. It
  goes into the checkout's build root, never the store workspace.
  `just zephyr build-module-fixtures` ties these together.
- **CI.** The gate job `zephyr-module-fixtures` is path-gated like the other
  narrow lanes and listed in `CI`. It runs in the Zephyr CI container and does
  `just setup zephyr` and then `just zephyr build-module-fixtures`.
- `sanitize_board` uses `flatten_board_id`.

## Measured

Host 241, Zephyr 3.7.0, SDK 0.16.9, `just zephyr build-module-fixtures`:
the hal_nxp fetch took 5 s and 148 MB. `zephyr.elf` linked with RAM
4,851,076 B of the 7 MiB `sram2` (66.09 %); `text` 1,103,460, `data`
32,018, `bss` 3,717,652. `CONFIG_NROS_RMW_CYCLONEDDS=y`. The board has no
flash region, because it runs from SRAM.

## Out of scope

The FreeRTOS RTD variant (`s32z270-freertos-rtd`) stays in
[#2632](../2632-s32z270-freertos-rtd-variant.md).
