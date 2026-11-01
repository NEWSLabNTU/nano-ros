---
rfc: 0114
title: "Board support registry: primitives and derivation"
status: Draft
since: 2026-10
last-reviewed: 2026-10
implements-tracked-by: [phase-522]
amends: [RFC-0064]
supersedes: []
superseded-by: null
---

# RFC-0114 — Board support registry: primitives and derivation

**Status:** Draft (2026-10-31). Written from the adoption discussion of
2026-10-31 (three purposes: industrial, field solutions, education) and
before any code. Implemented by
[phase-522](../roadmap/archived/phase-522-support-registry-primitives.md). The
companion RFCs of the same discussion are RFC-0115 (emulator-first board
policy) and RFC-0116 (adoption purposes and front doors); this one is the
substrate both of them record evidence into.

## Summary

`packages/boards/board-support.toml` becomes the one place a board fact is
typed. Each row states what a board IS (its package, platform family and
authored names), WHERE its image runs (`runs`: none, host, emulator or
hardware, with the emulator or board named), HOW OFTEN that run happens
(`cadence`: merge, daily, weekly or manual, with a date on manual) and WHICH
board carries the family's breadth (`breadth = "witness"` or
`breadth = "inherits <witness>"`). Everything else that today restates a
board fact — the test matrix cells, the interop smoke rows, the emulator
fixture rows, the weekly probe workflow and its case table, the printed
run-command check of issue 2169, the book's support page, the per-board
evidence page, the procurement rows, `nros board list` and
`nros doctor --report` — is GENERATED from the registry by one script with
stamped outputs and held by one gate.

The tier numbers retire. A board's status is a sentence built from the
primitives ("Verified in emulator, daily", "Boots on hardware, manual,
2026-10-05", "Builds", "Not supported") and its breadth is a derived text
("C, C++ and Rust pubsub and service, zenoh and cyclonedds, humble and
jazzy; inherited from the ZephyrQemuCortexM witness, board-specific: boot
and serial link").

Three decisions:

- **D10** — support levels are three authored primitives plus one generated
  status word, not a tier number.
- **D11** — breadth is derived from the matrix cells and inherited from the
  family's witness board; an inheriting board carries smoke cells only.
- **D12** — the registry is the only primitive; every other surface is a
  generated view with a staleness gate.

## Problem

### Tier 1 versus tier 2 was cadence, not evidence

The registry's header defines tier 1 as "`just ci` exercises it, asserted
runtime coverage, a nightly lane builds it" and tier 2 as "asserted runtime
coverage plus a nightly lane, but not in `just ci`". The evidence is the same
(a Runtime cell in `matrix::CELLS`); the difference is WHEN the lane runs.
Tier 3 ("compiles; no CI lane can boot it") mixes two different facts: a
board nobody has booted (`s32z270-freertos`) and a board a maintainer boots
by hand on a model nano-ros can download (the FVP, `just zephyr
verify-fvp-runtime`, with a measured boot on 2026-09-06). A reader of the
book's "Board Support Tiers" page cannot tell the two apart, and cannot tell
"runs on a host executable" (native_sim, threadx-linux) from "runs under
QEMU" (mps2-an385) from "runs on silicon" (nothing in the table today),
although those are the three questions every adopter asks first.

The names hid it. "Tier 2" reads as "second class"; what it means is "a QEMU
board whose cells run nightly rather than on every merge". `nros-board-zephyr`
holds three rows at three tiers for one crate (native_sim at 1, the
Cortex-M QEMU witness at 2, the FVP at 3), which RFC-0064 already called
out as the registry keying on the wrong thing.

### A board fact is typed in five places

Adding the NuttX rv-virt Rust probe track on 2026-10-31 (issue 2178) touched:
the book chapter's `probe=` tags, `scripts/probe/run-bootstrap-probe.sh`'s
`case` table and its `want` list, `just/probe.just`, the
`probe-platforms.yml` matrix, and the `ROWS` table of
`every_probed_chapter_boot_line_is_the_printed_run_command` in
`packages/cli/nros-cli-core/src/cmd/build_next.rs`. The board itself is also
a `[[board]]` row in `board-support.toml`, a `PlatformId` and a set of
`cell(...)` lines in `packages/testing/nros-tests/src/matrix.rs`, a set of
`[[fixture]]` rows in `examples/fixtures.toml`, and an entry in
`nros-sdk-index.toml`. Each of those has its own drift gate, each gate was
written after a drift, and the gates themselves disagree in reach (issue
1226's shape: a gate that works is not a gate that runs). Issue 2177's
re-read on the same day found `nros-board-mps2-an385-zephyr` listed as a
Scaffold in the generated tiers page while being the very board the
ZephyrQemuCortexM witness runs under, because the registry keys by crate and
the Cortex-M row is attributed to `nros-board-zephyr`.

### The precedent that already works

Knobs had the same shape and were fixed the same way: RFC-0103 D2b made
`packages/tooling/nros-zephyr-build/knobs.toml` the one row per knob, and
`scripts/gen-knob-registry.py` generates the Rust table, the cmake list and
`zephyr/Kconfig`'s knob section from it, with a staleness gate that fails
when any generated view differs from a fresh run. Since then "keep them in
sync" has not appeared in a knob issue. This RFC applies the same rule to
boards.

## Design

### D10 — Support levels: three primitives and one generated status word

> **Amended by [RFC-0118](0118-target-execution-model.md) D4 (2026-11):**
> `emulator` is read from the board descriptor's `[board.run]`, not typed in
> the registry, because an out-of-tree board with no registry row still has to
> run. D12's rule (no board fact typed twice) then makes the registry read it.
> `runs`, `hardware`, `cadence` and `last_run` stay registry facts.
> phase-528 W2 carries the change with this RFC's generator. **Landed
> 2026-11-01:** `scripts/gen-board-support.py` reads each row's `emulator`
> from its descriptor (through `board_keys`, or the row's package directory),
> and `check-board-registry` refuses a row that types one while an in-tree
> descriptor exists for its board. Only an `external` row may type it.

Each `[[board]]` row carries, in place of `tier`:

| Field | Values | Meaning |
| --- | --- | --- |
| `runs` | `none`, `host`, `emulator`, `hardware` | Where the board's image executes in evidence nano-ros holds. `none` is a board nothing boots. `host` is a host executable (native_sim, threadx-linux, freertos-posix, linux). `emulator` and `hardware` name the machine in the next two fields. |
| `emulator` | `qemu:<machine>`, `renode:<platform>`, `wokwi:<board>`, `fvp:<model>` | Required when `runs = "emulator"`. Also allowed beside `runs = "hardware"` to record a twin that is NOT the evidence. |
| `hardware` | a board id | Required when `runs = "hardware"`; allowed beside `runs = "emulator"` to record silicon the emulator stands in for. |
| `cadence` | `merge`, `daily`, `weekly`, `manual` | How often the run in `runs` happens: a merge-gating lane, the nightly, the weekly probe, or a maintainer by hand. |
| `last_run` | an ISO date | Required when `cadence = "manual"`; refused otherwise, because a lane's last run is a question for the lane, not the registry. |
| `infra` | `true` | Replaces `tier = "infra"`: shared traits, family drivers, ABI mirrors, PACs. No other evidence field is allowed on an infra row. |

The status word is generated, never typed:

| Status word | Condition |
| --- | --- |
| **Not supported** | `runs = "none"` and the row has no build cell |
| **Builds** | `runs = "none"` and the row has at least one BuildOnly cell |
| **Boots, manual (date)** | `runs` is `emulator` or `hardware`, `cadence = "manual"` |
| **Verified in emulator (cadence)** | `runs = "emulator"`, cadence merge, daily or weekly |
| **Verified on host (cadence)** | `runs = "host"`, cadence merge, daily or weekly |
| **Verified on hardware (cadence)** | `runs = "hardware"`, cadence merge, daily or weekly (needs a hardware runner; no row qualifies today) |
| **Infrastructure** | `infra = true` |

A manual row whose `last_run` is older than the registry's `manual_stale_days`
(a top-level value, 120 to start) degrades to **Builds** in every generated
view, with the stale date shown. The row is not edited; the derivation
reports what the evidence is worth today. This is the honest form of what
RFC-0064 described as "nobody has booted it recently unless a maintainer did
so by hand".

The gate predicates become: `runs = emulator|hardware|host` requires at least
one Runtime cell for the row's platform; `runs = none` requires zero Runtime
cells; `cadence = merge` requires the platform in the gate lane's coordinate
file; `cadence = daily` requires a `nightly_token`; `cadence = weekly`
requires a probe track in the generated probe matrix; `cadence = manual`
requires `last_run`. Maintainer counts key on the status word: a Verified row
needs two named maintainers, a Boots or Builds row one, with the existing
grandfather baseline unchanged.

### D11 — Breadth is derived and inherited

Breadth is the set of matrix coordinates a board's platform family has
Runtime or BuildOnly cells for: features (pubsub, service, action, params,
lifecycle), languages, RMWs and ROS editions. It is read from
`matrix::CELLS` and `interop::CELLS`, never written in the registry.

Each platform family has one **witness** row (`breadth = "witness"`): the
board whose emulator or host executable runs the family's matrix. Every other
row of the family states `breadth = "inherits <witness matrix_platform>"`,
and the generator gives it exactly the family's **smoke** cells: the talker
boots, the transport link comes up, one message is received by a stock ROS 2
peer. The evidence page prints both halves: "breadth inherited from
ZephyrQemuCortexM: C, C++ and Rust pubsub at Runtime, service and action
BuildOnly; board-specific evidence: boot and serial link, weekly, in Renode".

This is the conciliation with the existing boards. Zephyr's breadth is
asserted once, on native_sim and the mps2_an385 QEMU witness; a Nucleo, an
Arduino or a Pico board on Zephyr adds one smoke cell in its own emulator and
one on its own hardware, and nothing else. FreeRTOS inherits through
`FreertosMps2`, NuttX through `NuttxArm`, ThreadX through `ThreadxLinux`. A
board whose code path differs from its family's witness (a different IP
stack, a different libc, a different pointer width) is a new witness, and
the registry says so in `breadth`.

### D12 — The registry is the only primitive

`packages/boards/board-support.toml` is the primitive. One script,
`scripts/gen-board-support.py`, reads it (and the board descriptors it names,
`nros-board.toml`, for the facts a descriptor already owns: platform, names,
`west_board`, `supported_netstacks`) and writes every view below. Each
generated file carries a stamp line naming the generator and the registry
hash; the `check-board-registry` gate (a fast-line recipe phase-522 W3 adds)
regenerates into a temporary directory and
fails on any byte difference, and refuses a board fact typed outside the
registry where it can detect one (a `cell(...)` line in `matrix.rs` outside
the generated region, a `probe-platforms.yml` matrix entry the registry does
not name, a `ROWS` entry with no row).

Generated views, with the hand-authored thing each replaces:

| View | Replaces | Shape |
| --- | --- | --- |
| `matrix::CELLS` rows per board | hand `cell(...)` lines in `matrix.rs` | a generated `include!` region; the witness rows come from the family's feature × language × RMW × edition table in the registry, inheriting rows get the smoke cells. Comments that carry history move into the row's `notes`. |
| `interop::CELLS` smoke rows | hand `ic(...)` lines | one row per inheriting board whose smoke needs a ROS 2 peer |
| emulator fixture rows in `examples/fixtures.toml` | hand `[[fixture]]` rows for emulator builds | the row names a coordinate and a locator port; build arguments come from the descriptor and the family defaults |
| `probe-platforms.yml` matrix and the probe case table | the `matrix:` list and `run-bootstrap-probe.sh`'s `case` | one JSON file the workflow and the script both read |
| the `ROWS` table of the issue 2169 test | hand table in `build_next.rs` | generated include |
| `book/src/reference/board-support-levels.md` | `board-support-tiers.md` | the status word, breadth text, cadence, emulator and hardware per row |
| `book/src/reference/boards/<name>.md` | nothing (new) | the per-board evidence page: status, breadth, last green per lane, emulator twin, hardware status, chapter links |
| `supported-boards.md` procurement rows | the hand-maintained table | rows for boards with a registry entry; the hand rows for parts with no package stay, marked as such |
| `nros board list` | reads descriptors only | adds status word and cadence |
| `nros doctor --report` | nothing (new) | prints the registry status of every board the workspace names |

Adding a board is one registry row plus its descriptor; removing one is one
row deletion; recording a first hardware run is editing `runs`, `hardware`,
`cadence` and `last_run` on one row. No board fact is typed twice.

What is NOT derived, deliberately: the board descriptor (`nros-board.toml`)
keeps the facts the BUILD needs (RFC-0049, RFC-0064), because a build must
not read a test registry; the registry references the descriptor and the
generator reads both. The hand-authored fixture rows whose build arguments
are not a function of descriptor plus family default (cmake `cmake_defs`,
per-row `env`) stay authored under an allowlist that only shrinks (see
Evaluation).

### Migration of today's rows

| Row (`crate`, `matrix_platform`) | Today | New fields | Status word |
| --- | --- | --- | --- |
| `nros-board-linux`, Linux | tier 1, host-executable | `runs = "host"`, `cadence = "merge"`, `breadth = "witness"` | Verified on host (merge) |
| `nros-board-zephyr`, ZephyrNativeSim | tier 1, host-executable | `runs = "host"`, `cadence = "merge"`, `breadth = "witness"` | Verified on host (merge) |
| `nros-board-threadx-linux`, ThreadxLinux | tier 1, host-executable | `runs = "host"`, `cadence = "merge"`, `breadth = "witness"` | Verified on host (merge) |
| `nros-board-freertos-posix`, FreertosPosix | tier 2, host-executable | `runs = "host"`, `cadence = "daily"`, `breadth = "inherits FreertosMps2"` | Verified on host (daily) |
| `nros-board-mps2-an385-freertos`, FreertosMps2 | tier 2, cross-run | `runs = "emulator"`, `emulator = "qemu:mps2-an385"`, `cadence = "daily"`, `breadth = "witness"`; weekly probe recorded as a second lane | Verified in emulator (daily) |
| `nros-board-mps2-an385`, QemuBaremetal | tier 2, cross-run | `runs = "emulator"`, `emulator = "qemu:mps2-an385"`, `cadence = "daily"`, `breadth = "witness"` | Verified in emulator (daily) |
| `nros-board-zephyr`, ZephyrQemuCortexM | tier 2, cross-run | `runs = "emulator"`, `emulator = "qemu:mps2-an385"`, `cadence = "daily"`, `breadth = "witness"`; the row's package becomes `nros-board-mps2-an385-zephyr`, the descriptor the witness actually builds with | Verified in emulator (daily) |
| `nros-board-nuttx-qemu`, NuttxArm | tier 2, cross-run | `runs = "emulator"`, `emulator = "qemu:virt-armv7a"`, `cadence = "daily"`, `breadth = "witness"` | Verified in emulator (daily) |
| `nros-board-nuttx-qemu`, NuttxRiscv | tier 2, cross-run | `runs = "emulator"`, `emulator = "qemu:rv-virt"`, `cadence = "daily"`, `breadth = "inherits NuttxArm"` | Verified in emulator (daily) |
| `nros-board-threadx-qemu-riscv64`, ThreadxRiscv64 | tier 2, cross-run | `runs = "emulator"`, `emulator = "qemu:rv-virt"`, `cadence = "daily"`, `breadth = "inherits ThreadxLinux"` | Verified in emulator (daily) |
| `nros-board-zephyr`, Fvp | tier 3, hardware, `real_hardware = false` | `runs = "emulator"`, `emulator = "fvp:FVP_BaseR_AEMv8R"`, `cadence = "manual"`, `last_run = "2026-09-06"`, `breadth = "inherits ZephyrQemuCortexM"` | Boots, manual (2026-09-06); degrades to Builds when stale |
| `nros-board-s32z270-freertos` | tier 3, hardware, `real_hardware = true` | `runs = "none"`, `hardware = "s32z270"`, `breadth = "inherits FreertosMps2"` | Builds |
| S32K344 safety island (no row today) | — | new row: `runs = "hardware"`, `hardware = "mr-canhubk344"`, `cadence = "manual"`, `last_run` = the island's last measured boot (phase-474), `breadth = "inherits FreertosMps2"` | Boots on hardware, manual (dated) |
| `nros-board-esp32-qemu`, Esp32Qemu | scaffold | `runs = "none"`, no cells | Not supported |
| `nros-board-mps3-an536-freertos` | scaffold | `runs = "none"` | Not supported |
| `nros-board-qemu-cortex-a53` | scaffold | `runs = "none"`; issue 2177's branch already moves it to infra as a descriptor directory, and that ruling wins if it lands first | Not supported, or Infrastructure |
| `nros-board-mps2-an385-zephyr` | scaffold | becomes the package of the ZephyrQemuCortexM row above (issue 2177 re-read: it IS the witness) | Verified in emulator (daily) |
| `nros-board-common`, `nros-board-cffi`, `nros-board-freertos`, `nros-board-nuttx`, `nros-board-threadx`, `nros-board-threadx-port-riscv64`, `mps2-an385-pac`, `linux`, `zephyr` | infra | `infra = true` | Infrastructure |

`execution_class` retires: `host-executable` is `runs = "host"`, `cross-run`
is `runs = "emulator"`, and `hardware` split into the FVP (an emulator run by
hand) and the two silicon rows. `real_hardware`, `book_run` and `book_flash`
stay, and the book-section check moves into `check-board-registry`.

### Who reads the evidence page

The three adoption purposes of the 2026-10-31 discussion read the same page
for different reasons. An industrial adopter reads status, cadence and last
green to decide what to qualify; an educator cites the status word in a
syllabus and reads cadence to know whether a lab will still work next
semester; a field-solutions technician reads "Verified in emulator" and the
emulator name to know what to play with before hardware arrives, and the
hardware status to know what the flash line is worth. One page, three
readers, no tier number to decode.

### What this amends and what it leaves alone

- **RFC-0064 § "The supported matrix"**: the tier definitions, the
  "Consequence for the tier registry" paragraph and the witness table are
  superseded by D10 and D11. RFC-0064's stance (a board is a package; the
  seam is the ABI plus capabilities) is unchanged.
- **RFC-0112 framework support tiers** are a different axis (frameworks such
  as RTIC and PX4, not boards) and are owned by another session's
  phase-521 work. This RFC does not change them. When RFC-0112 is next
  revised, the same primitives (`runs`, `cadence`, breadth) are the suggested
  replacement for its tier numbers, so a reader sees one vocabulary.
- **RFC-0051** (one declared test matrix) is implemented one step further:
  the matrix is still one declaration, now generated from the board registry
  plus the family tables rather than typed in Rust.

## Evaluation and decision steps

Each decision is validated before the next one is relied on, and each has a
stated reversal.

1. **Round-trip first, change nothing.** The generator's first milestone is
   to regenerate today's `matrix.rs` cell region, `fixtures.toml` emulator
   rows, `probe-platforms.yml` matrix, the probe case table and the 2169
   `ROWS` byte-identical from a registry that still carries the OLD fields
   plus the new ones. Evidence: a clean `git diff` after `gen-board-support.py
   --write` on the current tree. Decision: if the round-trip needs more than
   an allowlist of authored fixture rows, D12 narrows to "cells, probe
   matrix, ROWS and book pages are generated; fixture rows stay authored and
   are checked, not written", and the RFC records the narrowing.
2. **The gate catches a hand-typed fact.** Mutation test: add a `cell(...)`
   line outside the generated region, add a probe matrix entry with no row,
   change a `ROWS` tuple; `check-board-registry` must fail on each with a
   message naming the row to edit. Decision: a mutation the gate cannot see
   is either made impossible by the file layout (the region is the whole
   file) or named as a known gap in the gate's own output.
3. **Status words match the maintainers' reading.** The generated
   `board-support-levels.md` is reviewed row by row against the migration
   table above before `board-support-tiers.md` is removed; every row whose
   word surprises a maintainer is a registry edit or a predicate fix, not a
   book edit. Decision: if two rows need a status word the table lacks, the
   word is added to D10 and the RFC amended; a one-off is a registry fix.
4. **Breadth inheritance is measured once.** For each inheriting board, the
   smoke cell runs and the claim "code path identical to the witness" is
   checked by diffing the two images' nano-ros object set (same crates, same
   features, same knobs; `mem-report --json` on both). Decision: a board
   whose set differs becomes a witness, not an inheritor.
5. **The manual-stale degradation is tested on the FVP row.** Set
   `manual_stale_days` below the FVP's age and confirm the page shows
   "Builds (last booted 2026-09-06)"; set it above and confirm "Boots,
   manual". Decision on the number itself (120 days to start) is the
   maintainer's and is a registry value, not a code constant.
6. **Reversal of D12 as a whole.** If after phase-522 W3 the allowlist of
   authored fixture rows is larger than the derived set and shows no path to
   shrinking, the registry stays the primitive for status and breadth only,
   fixture rows return to RFC-0051's authored shape, and this RFC is amended
   to say so. The status words (D10) and the inheritance rule (D11) do not
   depend on that outcome.

## Open questions

- Whether `nightly_token` stays an authored field or is derived from the
  platform family (every family has exactly one today).
- Whether the per-board evidence page should also carry the measured
  numbers RFC-0115 asks for (time to first output on that board, install
  minutes), or link to where the probe records them.
- How a downstream board package (RFC-0064's out-of-tree BSP) publishes its
  own registry row so `nros board list` shows the same status words for a
  board nano-ros does not ship.

## Decision log

- 2026-11-01 — Where the clock is applied. D10 says a stale manual row reads
  Builds in every generated view. The committed views (the book's markdown
  pages, the status file) carry no clock: a tracked file that changed on a day
  nobody edited anything would fail `check-board-registry` for every pull
  request from the stale day on. They print the row's current word and the
  date it starts reading Builds. Every view a reader sees applies today's date:
  `nros board`, `nros doctor --report` and `check-board-tiers.py --print-status`
  read the clock, and the rendered book does too, through the mdBook
  preprocessor `scripts/mdbook-board-status.py`, which re-renders each board
  page with the generator's own renderer when the book is built. The maintainer
  chose this over a scheduled regeneration job because the published page then
  states the fact on the day it is read. The preprocessor's self-test, run by
  `check-board-registry`, renders the FVP row on both sides of its stale date.
- 2026-11-01 — Evaluation step 1's narrowing applied by phase-522 W2. The
  emulator platforms carry 146 `examples/fixtures.toml` rows
  (`[[fixture]]` and `[[workspace_fixture]]` on freertos, threadx-riscv64,
  baremetal, nuttx, nuttx-riscv and zephyr-cortex-m); 68 of them carry per-row
  `cmake_defs`, `env` or `features` that no descriptor field explains, and the
  rest still name a per-row `dir`, `build_subdir` and locator port. No family
  default exists to derive them from, so the allowlist would be larger than
  the derived set with no path to shrinking. Fixture rows therefore stay
  authored and are CHECKED against the registry (a row's board is a registry
  row, its coordinate is in the cell table), not written by the generator. The
  `interop::CELLS` smoke rows stay authored for the same reason: each binds a
  peer, a direction and a build recipe the registry does not describe. The
  cells (`[[cell]]` in `packages/boards/board-support-cells.toml`, named by the
  registry's `cell_table`), the probe tracks (`[[probe]]`), issue 2169's
  `ROWS` and the book pages are generated, as D12 says.
- 2026-11-01 — phase-522 W1 migrated the rows. Three readings differ from the
  migration table above, each a registry fact rather than a new status word:
  `nros-board-mps3-an536-freertos` reads Builds (two fixture rows build it,
  and a fixture row naming the board counts as build evidence, which is also
  what gives `nros-board-s32z270-freertos` its Builds); the S32K344 island
  inherits ZephyrQemuCortexM, because its image is Zephyr; and a manual row
  needs no Runtime cell, since its evidence is the dated run.

- 2026-10-31 — Drafted after the adoption discussion. The maintainer ruled
  that support is named by evidence, not by tier number; that existing
  boards are reconciled rather than replaced (breadth per family, smoke per
  board); and that there is one source of truth for board support and the
  tests that prove it, so board packages, test scripts and docs stop being
  kept consistent by hand.
- 2026-11-01 — RFC-0118 D4 amends D10: `emulator` moves to the descriptor's
  `[board.run]` and the generator reads it there (phase-528 W2).
