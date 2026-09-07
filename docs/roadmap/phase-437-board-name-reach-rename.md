# Phase 437 — rename every board to the reach its build has

**Status (2026-09-08). PLANNED.** Implements
[RFC-0093](../design/0093-board-naming-states-reach.md). Nothing has moved yet;
the measurements below are from `main` at `9c0702322`.

Phase-422 W7 made both board vocabularies RESOLVE and deferred the question of
which is canonical. RFC-0093 answers it with a rule rather than a house style:
a board name states the reach of its build. This phase applies the rule — to
the index, the cmake overlays, the board crates, the example directories, the
fixture coordinates and the docs — and lands a gate so the two vocabularies
cannot grow back.

## Why this is a phase and not a `sed`

Nothing in the tree keys on a board name as a string: the test harness resolves
emulators by TOOL (`qemu_system_riscv32_path()`), never by parsing a board. So
every rename is mechanical, and that is exactly the hazard. A mechanical change
across ~600 files, six directory moves and five namespaces has no single point
where it can be reviewed for meaning, and a half-applied one leaves the tree
with THREE vocabularies instead of two.

The staging below exists so that each wave is separately verifiable and each
leaves the tree green.

## The blast radius, measured

Tracked files containing each name, and tracked PATHS carrying it as a segment:

| name | files | paths |
| --- | ---: | ---: |
| `mps2-an385` | 572 | 63 |
| `mps2-an385-freertos` | 232 | 17 |
| `qemu-arm-nuttx` | 177 | 118 |
| `qemu-riscv64-threadx` | 179 | 122 |
| `qemu-arm-freertos` | 188 | 117 |
| `qemu-arm-baremetal` | 172 | 107 |
| `qemu-esp32-baremetal` | 111 | 17 |
| `nuttx-qemu-arm` | 107 | 7 |
| `riscv64-qemu` | 68 | 1 |
| `qemu-riscv-nuttx` | 36 | 6 |
| `nuttx-qemu-riscv` | 32 | 1 |
| `esp32-c3` | 31 | 2 |

Six `examples/<board>/` directories move: `qemu-arm-baremetal`,
`qemu-arm-freertos`, `qemu-arm-nuttx`, `qemu-riscv-nuttx`,
`qemu-riscv64-threadx`, `qemu-esp32-baremetal`.

Five namespaces carry a board name, and all five must move together or the
resolution `check-board-vocabulary` asserts breaks:

1. `[board.*]` in `nros-sdk-index.toml` — what `nros setup <board>` looks up
2. `cmake/board/nano-ros-board-<name>.cmake` — what the BUILD includes
3. `packages/boards/nros-board-<name>` — the board crate
4. `NANO_ROS_BOARD` in `examples/fixtures.toml` — the test coordinate
5. `board=` in 50 `package.xml` exports — what a workspace DECLARES

## Work items

### W1 — the gate, before any rename

A rename verified only by `check fast` is a rename verified by nothing: the
gates that exist assert a board RESOLVES, not that its name is true. So the
gate comes first, in ratchet form (record today's violations, refuse new ones),
and each later wave shrinks its baseline.

`check-board-name-reach` asserts, per RFC-0093 §5:

* a board whose cmake overlay or board crate pins a linker script, a memory
  base or a peripheral address is a **part or machine** board — its name must
  not be an arch or ISA alone (`riscv64-qemu` fails);
* a board that pins none of those is a **system** board — its name must not
  contain a part or machine (`freertos-posix`, `threadx-linux` pass);
* no board name contains `qemu` unless upstream's own name does
  (`CONFIG_ARCH_BOARD="qemu-armv7a"` passes; `qemu-arm-freertos` fails).

It reads the pins rather than a list: `.ld`/`.lds`/`.x` in the overlay or crate,
`ORIGIN`/`RAM_START`/a hex MMIO literal. Self-testing, on the fast line.

**Acceptance.** The gate reproduces the RFC's table — every name it calls false
is one §4 renames, and every name §4 leaves alone passes.

### W2 — the fifth duplicate, and the marker the gate keys on

`check-board-vocabulary`'s mirror assertion keys on a `# = [board.X]` comment.
Comparing every `[board.*]` BODY, four pairs are byte-identical and only three
carry the marker: **`qemu-arm-baremetal` / `mps2-an385` has none.** Two names,
one build, nothing holding them in step.

Before renaming anything, either mark it or prove the two are not the same
board. Identical fields is evidence, not proof — and this is the one pair no
comment documents, so the evidence is all there is.

**Acceptance.** Every identical-body pair is either marked and asserted, or
shown to be two boards that coincide. `native`/`posix` stay unmarked and
deliberately so (`check-host-platform-vocabulary` owns them).

### W3 — the two correct names that are not index keys

`mps3-an536-freertos` and `s32z270-freertos` already obey RFC-0093 and
`nros setup <board>` cannot provision either. `freertos-posix` is a third:
a system board with a cmake overlay and no index entry.

This is not part of the rename — it is the reach rule finding three boards the
index cannot reach at all, and it is cheap.

**Acceptance.** `nros setup mps3-an536-freertos` / `s32z270-freertos` /
`freertos-posix` each resolve to a package set. `s32z270-freertos` is REAL NXP
silicon, so this is also the first index entry that is not an emulator.

### W4 — the machine boards

`qemu-arm-nuttx` + `nuttx-qemu-arm` -> **`qemu-armv7a-nuttx`**
`qemu-riscv-nuttx` + `nuttx-qemu-riscv` -> **`rv-virt-nuttx`**
`qemu-riscv64-threadx` + `riscv64-qemu` -> **`rv-virt-threadx`**

Two names collapse to one each. The first two borrow NuttX's own
`CONFIG_ARCH_BOARD`; the third has no upstream name to borrow, because ThreadX
ships no board layer — every address is a literal in
`packages/boards/nros-board-threadx-qemu-riscv64`, which is its own finding.

The two `rv-virt-*` entries are the same QEMU machine at different ISA widths,
distinguished by the `arch` field (`riscv32` vs `riscv64`) rather than by the
name — RFC-0093 R5.

**Acceptance.** `just nuttx`, `just threadx_riscv64` and their fixture rows
build and run at the new names; the old names resolve nowhere.

### W5 — the part boards

`qemu-arm-baremetal` + `mps2-an385` -> **`mps2-an385-baremetal`**
`qemu-arm-freertos` + `mps2-an385-freertos` -> **`mps2-an385-freertos`**
`qemu-esp32-baremetal` + `esp32-c3` -> **`esp32-c3-baremetal`**

W2 must have settled the first pair before this wave touches it.

`esp32-c3` is the rule's own witness: its cmake overlay is *"board overlay for
ESP32-C3 (real silicon) + ESP32-C3 QEMU"* — one name, both, because the name is
the part. Renaming the index key to match is the smallest change here and the
clearest.

**Acceptance.** `just esp32`, `just freertos` and the mps2 bare-metal fixtures
build and run at the new names.

### W6 — the directories

Six `examples/<board>/` moves, the board crates under `packages/boards/`, and
the `cmake/board/nano-ros-board-*.cmake` files.

Last on purpose: a directory move is the change most likely to strand a
reference, and by this point every namespace that NAMES a board has already
moved, so what remains is paths. `git mv` per directory, one commit each, so a
bisect can land between them.

**Acceptance.** `git grep` for each retired name returns nothing outside
`docs/**/archived/` and this phase doc. Fixture builds pass for every moved
leaf.

### W7 — the book and the retired-name ledger

The book, `AGENTS.md` and the RFCs name boards in prose. A doc that names a
board nobody can type is worse than one that omits it.

Retired names get a ledger entry, not silence: someone reading a two-month-old
issue needs `qemu-arm-freertos` to lead them to `mps2-an385-freertos`. The
`docs/issues/**/archived/` convention already accepts a stale link; live docs do
not.

**Acceptance.** `check-markdown-links` and `check-doc-refs` green; every
retired name appears in the ledger with its replacement.

## Non-goals

**Config axes are not board names.** `fvp-aemv8r-smp` folds SMP into the name,
and `nros-board-nuttx-qemu` carries `arm`, `arm-smp` and `riscv` defconfigs
where the first two are one machine in UP and SMP form. SMP is an axis. RFC-0093
R1 says nothing about it and this phase must not quietly decide it — a rename
that also re-cuts an axis is two changes wearing one commit.

**`deploy=` is untouched.** A deploy names a FAMILY and the `board=` beside it
selects the implementation; RFC-0087 D3 and `check-board-alias-unique` own that.

**`native` / `posix` / `linux` are untouched.** CLAUDE.md settled ROLE vs REACH
for those and `check-host-platform-vocabulary` enforces it.

**No behaviour changes.** Not a single package set, linker script or defconfig
moves in this phase. If a board's REACH turns out to be wrong, that is a
separate finding and a separate phase — the name follows the build, never the
other way round.

## Acceptance for the phase

* `check-board-name-reach` green with an EMPTY baseline (W1 records the
  violations; W4–W6 remove them).
* Sixteen `[board.*]` keys become twelve, plus three added by W3 — and no two
  bodies are byte-identical except `native`/`posix`.
* `git grep <retired name>` is empty outside archived docs and the ledger.
* Tier-2 fixtures build and run at the new names, on a lane that was green
  BEFORE the rename started (phase-413 W2's lanes are the precondition — a
  rename verified in a red lane is verified by nothing).
