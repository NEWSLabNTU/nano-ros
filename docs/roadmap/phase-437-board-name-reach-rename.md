# Phase 437 — rename every board to the reach its build has

**Status (2026-09-08). W1-W3 LANDED. W4-W6 are BLOCKED on two findings recorded below; W7 follows them.** Implements
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

**LANDED.** `check-board-name-reach`, on the fast line, ratcheted at **nine**
violations — exactly the nine names §4 retires, and nothing else:
`qemu-arm-baremetal`, `qemu-arm-freertos`, `qemu-arm-nuttx`, `qemu-riscv-nuttx`,
`qemu-riscv64-threadx`, `qemu-esp32-baremetal`, `nuttx-qemu-arm`,
`nuttx-qemu-riscv`, `riscv64-qemu`. `mps2-an385*`, `mps3-an536-freertos`,
`s32z270-freertos`, `esp32-c3`, `freertos-posix`, `threadx-linux`, `native`,
`posix`, `zephyr` and `fvp-aemv8r-smp` all pass.

Two things the implementation had to get right, both found by running it:

**Reach is read from the OVERLAY alone.** The first version walked the
`packages/boards/**` directories each overlay names and looked for linker
scripts and MMIO literals. That is wrong, because those directories are
SHARED — `nros-board-freertos` is referenced by four boards and
`nros-board-common` by two — so the FreeRTOS Cortex-M linker script made
`freertos-posix` look pinned, and a peripheral address in `threadx_hooks.c` did
the same to `threadx-linux`. Both of those travel. Attributing a shared crate's
pins to every consumer inverts the answer for exactly the boards the rule is
about.

**The gate is conservative in one direction only.** A board whose toolchain
comes from an external SDK (ESP-IDF supplies ESP32-C3's linking) pins nothing in
its overlay and reads as a system board — so the gate can call a target board
portable, never the reverse. That is safe: the SYSTEM rule then forbids naming a
machine or an emulator, which is what catches `qemu-esp32-baremetal` either way.

**Not enforced: R2** (`<where>-<stack>`, stack last). `mps2-an385` and
`esp32-c3` carry no stack suffix and pass the gate, while §4 still renames them.
R1/R3/R4 are about a name stating something FALSE; a missing stack suffix
misleads nobody. Enforcing R2 would need `native`, `posix`, `zephyr` and
`fvp-aemv8r-smp` carved out, and an allow-list carved out of a shape rule is how
a gate stops meaning anything. R2 stays this phase's job, in W5.

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

**LANDED — ONE board, and the table said so all along.** The board table's own
header reads: *"Keyed by the canonical board id (`examples/<board>/` dir name
OR the board-crate name)"*. That is the doubling rule, written in the contract.
`qemu-arm-baremetal` is the examples directory; `mps2-an385` is the crate. Both
were added by `719ff9436`, adjacent, with identical bodies — born together, not
one mirrored onto the other later, which is why the pair predates the `# =`
convention and never got a marker.

The same commit is the control: it also added `[board.stm32f4]` with
`cortex-m4` and `openocd` instead of `qemu`, because that one is real hardware
you flash. The author DID encode the difference when there was one.

Marked rather than collapsed, so `check-board-vocabulary` holds the two in step
for the waves before W5 merges them. The gate now asserts **five** mirrored
pairs.

Two findings this wave turned up that later waves must not trip over:

* **`cmake/board/nano-ros-board-mps2-an385.cmake` has zero live consumers.**
  `NANO_ROS_BOARD` is set to `mps2-an385` nowhere in the tree; the only
  occurrences are a `FATAL_ERROR` example string and a doc reference. Its own
  header is also stale — it claims it is used under
  `NANO_ROS_PLATFORM=freertos`, but `nano-ros-board-mps2-an385-freertos.cmake`
  is fully self-contained. So the overlay is bare-metal-only, and a
  `-baremetal` suffix is a correction rather than a narrowing.
* **W6's acceptance needs namespace scoping for this pair.** `git grep
  qemu-arm-baremetal` can go empty; `git grep mps2-an385` cannot and must not.
  The string survives legitimately as the QEMU machine argument
  (`-machine mps2-an385`), as the deploy-token alias this phase's non-goals
  leave untouched, as the linker script `mps2-an385.x`, and as the
  `mps2-an385-pac` crate. Only the INDEX KEY retires.

### W3 — the two correct names that are not index keys

`mps3-an536-freertos` and `s32z270-freertos` already obey RFC-0093 and
`nros setup <board>` cannot provision either. `freertos-posix` is a third:
a system board with a cmake overlay and no index entry.

This is not part of the rename — it is the reach rule finding three boards the
index cannot reach at all, and it is cheap.

**Acceptance.** `nros setup mps3-an536-freertos` / `s32z270-freertos` /
`freertos-posix` each resolve to a package set. `s32z270-freertos` is REAL NXP
silicon, so this is also the first index entry that is not an emulator.

**LANDED.** All three resolve — 6, 5 and 3 packages. Every field measured
rather than copied:

* `mps3-an536-freertos` — `arch = "cortex-r52"` from
  `cmake/toolchain/arm-freertos-armcr52.cmake` (`CMAKE_SYSTEM_PROCESSOR
  cortex-r52`, `-mcpu=cortex-r52`), and the pinned QEMU lists
  `mps3-an536  ARM MPS3 with AN536 FPGA image for Cortex-R52`. Package set is
  identical to `qemu-arm-freertos` and only `arch` differs, so it adds no new
  byte-identical body and no new mirror obligation.
* `s32z270-freertos` — **no `qemu`, deliberately.** No emulator models this
  SoC (`board-support.toml`: tier 3, `execution_class = "hardware"`, no
  `matrix_platform`), so listing one would advertise a run path that does not
  exist. What it provisions is what makes a clean checkout LINK, which is the
  cell's whole promise. Three things it also needs are NOT index-expressible
  and stay consumer-provisioned seams: NXP's RTD NETC driver (NXP
  Confidential, arriving as a strong override of the board's weak fail-loud
  `nros_board_register_netif`), NXP's `GCC/ARM_CR52_GIC` FreeRTOS port, and an
  RTD PBcfg set. `[gated.*]` exists for license-gated packages and holds zero
  entries; none of the three has a public URL.
* `freertos-posix` — `platform = "freertos"`, not `posix`.
  `nros_entry_lower::board_family` puts it in `BoardFamily::Freertos` with a
  note that it once fell through to the host default, *"a silent wrong answer
  that only surfaced at link when `app_main` came up undefined"*. The key
  spelling is fixed by its use as a LANE coordinate.

This is the first index entry for real silicon, and the first whose package set
is deliberately incomplete — both stated in the entry rather than left to be
rediscovered.

## BLOCKED: what a full site survey found, and why W4-W6 do not start yet

A read-only inventory of all five namespaces was taken before renaming anything.
It found two things that make the remaining waves a different job from the one
this doc scoped, and both are recorded here rather than discovered halfway
through a 600-file sweep.

### Finding 1 — the RFC's "nothing keys on the string" is true of one path and false of five

RFC-0093 §6 says the rename is mechanical because nothing keys on a board name;
the evidence given was the test harness resolving emulators by TOOL. That holds
for that path and for **eight** other derivation shapes it does not:

1. **The fixture group key IS the platform string, and IS a directory name.**
   `scripts/build/fixtures-target-dir.sh:100` declares
   `NROS_FIXTURE_SHARED_PLATFORMS="... qemu-arm-baremetal ... qemu-esp32-baremetal"`
   — **two retired names** — and the resulting `build/cargo-fixtures/<platform>`
   path is hardcoded independently in `just/qemu-baremetal.just`,
   `scripts/check-weak-symbols-image.sh` and asserted in
   `nros-tests/src/fixtures/groups.rs`. Rename a `platform =` value without
   moving all of them and the group dir is valid-looking and EMPTY: no error,
   just missing fixtures — the phase-340 P2 regression exactly.
2. **The overlay path is built from `NANO_ROS_BOARD`** in all four platform
   dispatchers (`include(".../nano-ros-board-${NANO_ROS_BOARD}.cmake")`), so a
   grep for the old name finds neither the include nor the file.
3. **The cmake board vocabulary is derived from filenames** —
   `check-board-vocabulary` lists `cmake/board/` and strips the prefix. It
   FOLLOWS a rename rather than catching a half-applied one.
4. **The crate vocabulary is derived by prefix-strip in five places**, and at
   runtime `board_descriptor.rs::directory_alias()` accepts the crate DIRECTORY
   as an alias. Moving a board crate silently changes an alias the resolver
   honours, with no grep hit anywhere.
5. **`board_family` falls through to the host** (`_ => BoardFamily::Native`).
   Loud eventually — `app_main` undefined — but many minutes and one build
   stage from the rename, and the same file records `freertos-posix` having
   fallen through this way before.
6. Board-key match arms in `nros-orchestration-ir` (fail loud), plus a
   hand-written `known_boards_csv()` shown in a user-facing error.
7. Board id as a data-file lookup key: 16 `system.toml` sites, per-board dicts
   in `check-site-config.py` and `check-stack-floor.py`, `matrix.rs` arms, a
   cmake PRESET filename derived from the id.
8. Path interpolation from the token — `format!("examples/qemu-arm-nuttx/rust/{role}")`
   and friends, `dep-chain-check.sh`'s `CELLS` array, and a fail-OPEN fallback
   in `binaries/mod.rs` that returns `<dir>/target` when a leaf lookup misses,
   so a half-applied rename reads as "binary not prebuilt".

Plus two that fail by NOT firing: path-exclusion regexes in `justfile`/`just/*`
that stop excluding, and five `paths-filter` globs in `nightly.yml` — a stale
glob means the lane never triggers and goes green by not running.

**Consequence for W6.** "One `git mv` per directory, one commit each" is wrong
as written. The crate-dir move must land WITH the alias arrays, and the
`examples/<board>/` move must land WITH `NROS_FIXTURE_SHARED_PLATFORMS`, the
fixture `platform =` values and the three hardcoded `build/cargo-fixtures/`
paths — otherwise an intermediate revision builds, reports green, and produces
nothing.

### Finding 2 — `qemu-esp32-baremetal` wears three hats, and one is a non-goal

    git grep -hoP '^\s*deploy\s*=\s*"\K[^"]+' -- '*.toml' | sort | uniq -c
      2 qemu-esp32-baremetal

It is simultaneously an index `[board.*]` key, a fixture `platform` value and a
member of `NROS_FIXTURE_SHARED_PLATFORMS`, **and a live `deploy=` token in two
manifests**. This phase's non-goals say `deploy=` is untouched. So W5 cannot
rename it without either leaving the two spellings disagreeing or breaking a
stated non-goal.

That is a decision, not a detail, and it belongs to RFC-0093 rather than to a
sweep: either the RFC's §4 row for esp32 is deferred, or the non-goal is
amended to say a deploy token that IS a board key moves with it. **W5 does not
start until that is answered.**

### The precondition this phase already stated, and which is not met

> Tier-2 fixtures build and run at the new names, on a lane that was green
> BEFORE the rename started.

Phase-413 W2's lanes are the precondition. As of 2026-09-08 `nightly` and
`run-matrix` have no post-W2 verdict at all and `host-tests` is red. Renaming
600 files into a lane with no signal capacity means a rename regression and the
lane's existing red are indistinguishable — which is the failure mode
`just nightly-triage` exists to name.

W4-W6 wait on: Finding 2's answer, and one green run of each lane.

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
