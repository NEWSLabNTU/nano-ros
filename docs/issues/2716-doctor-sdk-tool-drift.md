---
id: 2716
title: "`nros doctor` never compares the SDK store with the [tool.*] pins, so a stale tool runs silently"
status: open
type: bug
severity: medium
area: [tooling, setup]
related: [0466, 0493, 0500, 0929, 1273, 1413]
found: 2026-11-18
---

**Home:** no phase. Found on a contributor host whose every gate ran a
`sccache` two minor lines behind the pin.

## What happens

`nros-sdk-index.toml` pins each host tool (`[tool.<name>] version = ...`) and
`nros setup --tool <name>` installs it at `~/.nros/sdk/<name>/<version>`. The
store accumulates: a pin bump installs nothing until someone runs setup again,
and the old version stays where every consumer finds it.

Nothing tells the contributor. `nros setup --check` does walk `[tool.*]` and
prints `[MISSING] tool <name> <pin> (store holds: ...)`, but `just doctor` (what
`nros doctor` runs inside a workspace) keeps only that command's `[BROKEN]`
lines, and `nros doctor` itself never reads `[tool.*]` at all. So a pin that
moved past the store is invisible on the one surface a contributor runs to ask
"is my host right".

`[tool.sccache]` moved to `0.17.0-nros1` in 2e2c551d6d (2026-09-11). On the
host that found this, the store held only `0.8.2-nros1`, and every gate since
had run it.

## Measured drift on one host (store vs main)

| state | tools |
| --- | --- |
| stale (pin absent, older version installed) | `sccache` (want 0.17.0-nros1, have 0.8.2-nros1); `openocd` (want 0.12.0-nros2, have 0.12.0-nros1) |
| absent | `cargo-llvm-cov`, `cargo-show-asm`, `cargo-sweep`, `mdbook`, `renode`, `rustfilt` |
| at the pin | 16 tools |
| superseded versions still on disk | `arm-none-eabi-gcc` 13.2-nros1 / 13.2-nros4; `qemu` 11.0.0-nros2 / -nros6 / -nros7; `zephyr-sdk` 0.16.8 |
| store dirs no `[tool.*]` row names | `esp32-qemu`, `espflash`, `play_launch_parser`, `zenohd` |

(`[tool.X.source]` subtables are recipes, not tools, and are not counted.)

## Where the four unindexed directories are pinned

None of the four is pinned anywhere as a store tool any more. Each is residue of
a row that was deleted on purpose:

- **`zenohd`** -- `[tool.zenohd]` removed in bc9de35a9b (phase-362 W4,
  RFC-0075). The router is ROS's `rmw_zenohd`, shipped with `rmw_zenoh_cpp`;
  the only pin is the compatibility range `[compat.rmw_zenoh_cpp]`, which is
  observed, not provisioned. Nothing reads the store copy.
- **`play_launch_parser`** -- `[tool.play_launch_parser]` removed in 1c6ca2e35d
  (issue 1413). The parser is the crate `nros-launch-resolve` links statically;
  its one pin is the `packages/cli/third-party/play_launch` gitlink. Nothing
  reads the store copy.
- **`esp32-qemu`** -- `[tool.esp32-qemu]` removed in 1cfa392bb6 (phase-521 W6,
  the ESP32-C3 board retired). Pinned nowhere. Only a stale comment survives,
  in `scripts/lib/qemu-bin.sh` (the `noprefix` rung's example).
- **`espflash`** -- `[tool.espflash]` removed in the same commit 1cfa392bb6.
  Pinned nowhere, but **two callers still name it**: `just workspace
  cargo-tools` runs `nros setup --tool espflash` (non-fatal, so it now fails
  quietly on every `just setup base`), and `just workspace doctor` counts
  `espflash` absent from `PATH` as a missing cargo tool. Both are dead since the
  board left; they are a follow-up, not part of this fix.

`scripts/sdk-retired-tools.txt` (read by `just doctor`'s "retired SDK store
entries" warning, issue 0653) names only `zenohd`; the other three retired rows
never joined it. The new doctor section lists all four from the index alone,
because a store dir no `[tool.*]` row names is retired or foreign by
definition.

## Fix

1. **Required or opt-in comes from the index.** No such notion existed on
   `[tool.*]`: `just workspace setup` decides it in recipe bodies (some steps
   fatal, some `|| echo WARNING`), and `[board.*].packages` says what a board
   needs. Every `[tool.*]` row now carries `need = "required" | "opt-in"`
   (`ToolNeed` in `sdk_index.rs`), documented at the head of the tool section.
   `required` is what the gate runs whatever the scope: `cargo-nextest`,
   `clang-format`, `corrosion`, `make`, `ninja`. The rest are opt-in. The board
   notion is reused rather than repeated: under `nros doctor --board <b>`, the
   board's `packages` count as required too. A test holds every shipped row to
   carrying the field.
2. **One comparison.** `sdk_store::sdk_drift` classifies each row: at the pin,
   stale, missing-required, absent-opt-in, plus superseded versions beside a
   present pin and store dirs no row names. Presence is
   `sdk_store::pin_present`, which `nros setup --check` now calls too, so the
   two verbs cannot disagree; installed versions come from the existing
   `installed_versions` (provenance-marked dirs only). The index is read
   through the existing `SdkIndex::load` / `locate_index`.
3. **`nros doctor` reports it.** A stale tool, and a missing required tool,
   are a `[WARN]` line naming `nros setup --tool <name>`, the verb every
   other remedy in the tree uses. They do not count as problems. An absent
   opt-in tool is silent (counted in the summary only). Superseded versions
   and unindexed dirs are listed as information; nothing is pruned or deleted.

Re-provisioning `sccache` and `openocd` on the host that found this is held
for the owner's ruling; the doctor only reports.
