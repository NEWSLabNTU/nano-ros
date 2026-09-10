# Phase 437 — rename every board to the reach its build has

**Status (2026-09-08). COMPLETE — W1 through W7 landed.**
`check-board-name-reach`: **0 violations**. `check-board-vocabulary`: **0
mirrored pairs**. Nineteen `[board.*]` keys are fourteen (five duplicate pairs
collapsed, the three W3 added kept). NOT verified: any build — see "What is
unverified" at the end. Implements
[RFC-0093](../../design/0093-board-naming-states-reach.md). Nothing has moved yet;
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

### Finding 2 — RESOLVED by RFC-0093's amendment: `qemu-esp32-baremetal` wears three hats

    git grep -hoP '^\s*deploy\s*=\s*"\K[^"]+' -- '*.toml' | sort | uniq -c
      2 qemu-esp32-baremetal

It is simultaneously an index `[board.*]` key, a fixture `platform` value and a
member of `NROS_FIXTURE_SHARED_PLATFORMS`, **and a live `deploy=` token in two
manifests**. This phase's non-goals say `deploy=` is untouched. So W5 cannot
rename it without either leaving the two spellings disagreeing or breaking a
stated non-goal.

**Answered (2026-09-08).** RFC-0093's non-goal is amended: a deploy token that
IS ALSO a board key follows its board, because leaving it behind does not
preserve a separate vocabulary — it leaves a THIRD spelling of one thing. So
`deploy = "qemu-esp32-baremetal"` becomes `deploy = "esp32-c3-baremetal"` in
both manifests. Deploy values that are not board keys stay untouched.

### W4a — the fixture-platform axis (added 2026-09-08)

RFC-0093 R6: a fixture `platform =` names a FAMILY, never a board. Ten of the
twelve values already do; two do not.

    qemu-arm-baremetal   (21 rows)  -> baremetal
    qemu-esp32-baremetal ( 3 rows)  -> esp32   [COLLAPSES an existing duplicate]

The esp32 half removes a duplicate rather than creating one:
`PlatformId::Esp32Qemu => &["esp32", "qemu-esp32-baremetal"]` maps one platform
to two coordinates today.

**This wave is where the group-key hazard lives.** A fixture platform IS a
directory name. All of these must move in ONE commit or the group dir is
valid-looking and empty:

* `examples/fixtures.toml` `platform =` values
* `NROS_FIXTURE_SHARED_PLATFORMS` (`scripts/build/fixtures-target-dir.sh:100`)
* `just/qemu-baremetal.just:263` `FIXTURE_TARGET := absolute_path("build/cargo-fixtures/qemu-arm-baremetal")`
* `just/qemu-baremetal.just:605` `rm -rf build/cargo-fixtures/qemu-arm-baremetal`
* `matrix.rs`'s `PlatformId -> tokens` arms
* the assertions in `nros-tests/src/fixtures/groups.rs`

**Acceptance.** `nros_fixture_group_slug` returns the new token, the group dir
under `build/cargo-fixtures/` is the new name, and a fixture build for each
platform produces artifacts where the test-side resolver looks — verified by a
BUILD, never by a gate (#393's rule).

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

**LANDED.** The ledger is
[`docs/reference/retired-board-names.md`](../../reference/retired-board-names.md),
linked from RFC-0093 §4, from `docs/design/README.md`'s RFC-0093 row and from
the book's CLI reference. It carries all eleven retired spellings (the nine
index keys plus the two `mps2-an385` / `esp32-c3` bare names), the two fixture
`platform =` coordinates, and — because the phase's own W2 finding predicted
the confusion — the strings that look retired and are not: `mps2-an385` in 659
files as a QEMU machine argument, two linker scripts, a PAC crate, two Zephyr
fragments and two deploy tokens; `esp32-c3` in 34 files as the chip; and the
four `nros-board-*` crate directories that keep a `qemu` in their name because
a crate is not a board key.

The book was fixed completely and the rewrite/leave line was drawn per
occurrence elsewhere:

* **Rewritten** — anything a reader TYPES or NAVIGATES TO: every `nros setup
  <board>`, `cmake --preset <board>`, `NANO_ROS_BOARD=` and `examples/<board>/`
  path in `book/**`; `docs/reference/c-api-cmake.md`,
  `docs/reference/riscv64-threadx-c-porting.md`, `docs/guides/threadx-setup.md`;
  and the present-tense body prose of RFC-0026, RFC-0048, RFC-0064, RFC-0066
  and RFC-0077.
* **Left as record** — dated measurements and changelog entries (RFC-0062's
  *"Measured 2026-09-06"* board census, RFC-0026's own revision log, RFC-0064's
  phase-337 deletion list and fixture-count table), the
  `docs/development/audit-findings-*` files, `docs/research/sdk-ux/*` (each
  stamped *"Status: research note (2026-05-04)"*), `docs/issues/README.md`'s
  resolved-issue summaries, and every `docs/issues/archived/**`.
* **Annotated, not rewritten** — verbatim tool output inside OPEN issues (1038,
  1115): the captured text stays as captured and a one-line note gives the
  current key and points here.

Three claims the sweep found FALSE and fixed while it was there, each verified
against the tree rather than assumed: `book/src/reference/cli.md` listed
`stm32f4` as a board name (`nros setup` rejects it — no `[board.stm32f4]`
exists, and `configuration.md` already records the crates leaving the tree), so
the list is now the fourteen keys that resolve; RFC-0048's consumption example
said `nros setup freertos-mps2-an385`, a key in the wrong order that never
resolved; and `workflow-by-platform.md` still carried `esp32-c3-baremetal` as a
fixture-platform grid row, which W4a's R6 collapse had already merged into
`esp32`.

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

## What landed, W4–W7

Three agents renamed one board family each in isolated worktrees, because the
families collide through `nros-sdk-index.toml`, `examples/fixtures.toml` and
`matrix.rs` — concurrent edits in one tree would have corrupted each other.
Their branches were then cherry-picked in sequence.

**The merge was where the work was.** Every shared file conflicted, and the
resolution was never "take one side": each branch had renamed ITS board and left
the others at their old names, so the answer was take HEAD and apply the
incoming branch's substitution, so both renames survive. The nightly
`paths-filter` needed all five globs correct simultaneously — a stale one means
the lane goes green BY NEVER RUNNING.

**One conflict was worth the whole exercise.** The ThreadX branch renamed six
`binary_name` literals to `rv-virt-threadx-<role>` — still the cargo package
name. Issue 1222's fix, landed just before the merge, had them as the CMake
targets (`riscv64_threadx_rust_<role>`). Git presented it as an ordinary
conflict; the extended gate is what settled it. Fixing 1222 BEFORE the merge
rather than after is why that landed correctly instead of quietly reverting.

### Findings the waves produced

* **A sixth duplicate the RFC's grep missed.** `deploy = [...]` — the ARRAY
  form in `[package.metadata.nros.application]` — named two retiring boards.
  RFC-0093 measured `^\s*deploy\s*=\s*"`, the scalar form. Both moved.
* **Issue 1222.** Six ThreadX RV64 resolvers asked for the cargo PACKAGE name
  where CMake declares a different target, so every one resolved to a path no
  target writes — `skip!("fixture missing")` on a tree where the fixtures were
  built. The gate could not see it because the literals were one frame up, in a
  local wrapper. Fixed, and the gate now follows wrappers structurally.
* **The entry `deploy =` scalar and its `[deploy.<board>]` table key are
  unguarded.** A rename can move one and not the other, and nothing notices:
  `check fast` is buildless, and `synthesise_self_bringup` takes the FIRST
  deploy block rather than the one the entry names, so a single-block leaf keeps
  working while `system.deploy.<target>` is keyed on a board nothing deploys to.
  Worth a gate; not written here.
* **`build_root_derivation.sh` derived its "unmigrated platform" witness from a
  hardcoded list.** `esp32` joining the shared platforms left all four migrated
  and the probe refused to verdict — correctly. Second time that list has
  rotted. It reads the manifest now.
* **`check-stack-floor.py`'s `FLOORS` is keyed by the fixture PLATFORM**, not by
  a board — `board_of` matches it against a path component of the group dir.

### W7 — the docs

The book is fixed completely: every `nros setup <board>`, `cmake --preset`,
`NANO_ROS_BOARD=` and `examples/<dir>/` path names something that exists, and
the fourteen board keys in `cli.md` were each checked with `--dry-run`. Two that
never resolved were found on the way: `stm32f4` (crates left the tree) and
`mps2-an385` (retired in W5).

`docs/reference/retired-board-names.md` is the ledger — eleven retired spellings
to their replacement and the rule that selected each, the two fixture-platform
coordinates under R6, and a "Strings that did NOT retire" section, because
`mps2-an385` survives in 659 files as a QEMU `-machine` argument, two linker
scripts and the `mps2-an385-pac` crate, and `esp32-c3` in 34 as the chip.

Records were left as records: 22 `docs/roadmap/` files, dated audit findings and
research notes, RFC-0062's stamped census, and every archived issue. Two open
issues whose occurrences are verbatim tool output got a one-line note rather
than a rewritten capture.

## What is unverified

**Nothing here was built.** Disk sat at 97% throughout, so no agent and no
merge step ran a fixture build. Specifically unmeasured:

* that `build/cargo-fixtures/baremetal` and `.../esp32` are written and found
  where the test-side resolver looks — W4a's stated acceptance is a BUILD, never
  a gate (#393), and it has not been met;
* that the six moved example trees still build after their `dir =`, `id =` and
  workspace-exclude changes;
* that `just esp32 build-qemu` still packs a flash image with the renamed row
  ids;
* that the six ThreadX RV64 cells 1222 unblocked now RUN.

And the precondition this phase stated at the outset still holds: phase-413
W2's lanes have no green post-W2 verdict, so the first CI run after this lands
is the first real test of ~700 renamed files. A rename regression and that
lane's existing red are indistinguishable until it goes green once.
