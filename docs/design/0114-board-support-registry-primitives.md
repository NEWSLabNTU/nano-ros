---
rfc: 0114
title: "Board support registry: primitives and derivation"
status: Stable
area: platform-board-rtos
since: 2026-10
last-reviewed: 2026-11-14
implements-tracked-by: [phase-522, phase-550, phase-585]
amends: [RFC-0064, RFC-0051]
supersedes: []
superseded-by: null
---

# RFC-0114 — Board support registry: primitives and derivation

> **Stable (2026-11-14, phase-555 W3, user ruling P4 (c)).** phase-522
> shipped it. The three open questions are filed as
> [issue 2536](../issues/2536-board-registry-open-questions.md), and were
> ruled on 2026-11-17 (see "Open questions").

**Status:** Stable (2026-11-14); drafted 2026-10-31. Written from the adoption discussion of
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
`weekly.yml` matrix, and the `ROWS` table of
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
| `emulator` | `qemu:<machine>`, `renode:<platform>`, `fvp:<model>` | Required when `runs = "emulator"`. Also allowed beside `runs = "hardware"` to record a twin that is NOT the evidence. (`wokwi:<board>` was in this list until 2026-11-06, when RFC-0115 Amendment 1 ruled that a twin must be free and open source.) |
| `hardware` | a board id | Required when `runs = "hardware"`; allowed beside `runs = "emulator"` to record silicon the emulator stands in for. |
| `cadence` | `merge`, `daily`, `weekly`, `manual` | How often the run in `runs` happens: a merge-gating lane, the nightly, the weekly probe, or a maintainer by hand. |
| `last_run` | an ISO date | Required when `cadence = "manual"`; refused otherwise, because a lane's last run is a question for the lane, not the registry. |
| `infra` | `true` | Replaces `tier = "infra"`: shared traits, family drivers, ABI mirrors, PACs. No other evidence field is allowed on an infra row. |
| `flavour` | `std`, `no_std` | Only on an `external` row (its board package is in someone else's tree), and required there when the row has a `matrix_platform`: no board crate here can say the image's std flavour, so the row states it and `check-flavour-lanes` checks the stated value like a read one. Refused on every other row, whose flavour is read from its crate. Added 2026-11-07 (phase-542 W0.2). |
| `roads` | `[{ name = "<road>", lane = "<workflow>:<job>" }, …]` | Optional. The build roads a board's packages reach it by, each with the lane that keeps it. A row without it has ONE road, kept by its cadence's job. With it, exactly one entry has no `lane`: the default road, kept by that same job. Added 2026-11-13 (phase-550 W13); see the amendment below. |

**Amended 2026-11-04 by [RFC-0122](0122-behaviour-bringups-road-witnesses-cadence-backed-ci.md) D8/D9:** each `cadence` value names exactly
one workflow job that runs the board's cells at that cadence (`merge` is the
`merge-runtime` job on `merge_group`). A value with no such job is lowered,
not kept: the registry may not promise a cadence the CI does not run.

**Amended 2026-11-13 (`roads`, phase-550 W13):** a board may be reached by
more than one build road, and the registry names each with the lane that
keeps it. The first user is NuttX: the workspace road (`nros build`, a C/C++
package through CMake against the kernel's export, RFC-0130 D7) is the one
road, and the make app in NuttX's own apps tree is a GUEST road with its own
nightly job. The field is generic. A lane is bound both ways, like a cadence:
the named job must exist on a scheduled workflow and list `<row>:<road>` in
its `env: NROS_BOARD_ROADS`, and every such token in any workflow must be a
road some row lists with that job as its lane. `check-cadence-jobs` holds
both directions; the board's evidence page shows a "Build roads" line. The
roads add no status word: the status stays the cadence's, and a road is
evidence of reach, not of a run.

The status word is generated, never typed:

| Status word | Condition |
| --- | --- |
| **Not supported** | `runs = "none"` and the row has no build cell |
| **Builds** | `runs = "none"` and the row has at least one BuildOnly cell |
| **Boots, manual (date)** | `runs` is `emulator`, `hardware` or `host`, `cadence = "manual"` (`host` added 2026-11-07, phase-542 W0.1) |
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
file; `cadence = daily` requires a `nightly_token` (the `just` module that builds
and tests the board; since phase-558 the nightly runs one job per board, named
after its `matrix_platform`, so the token picks the verbs and the board names the
verdict); `cadence = weekly`
requires a probe track in the generated probe matrix; `cadence = manual`
requires `last_run`. Maintainer counts key on the status word: a Verified row
needs two named maintainers, a Boots or Builds row one, with the existing
grandfather baseline unchanged.

> **Ruled 2026-11-07 (user, phase-542 W0; see the decision log).** PX4 SITL's
> row (PR #3169) met three cases D10 did not cover:
>
> 1. **Host + manual** reads "Boots, manual (date)". No new word.
> 2. **An `external` row with a `matrix_platform` and no board crate** states
>    its `flavour` (field table above); `check-flavour-lanes` checks it, and
>    external rows are not excluded from the gate.
> 3. **An automated cadence for PX4** is backed by a real Runtime cell, not a
>    Gated one. That needs `nros_tests::alloc::domain_of`'s window narrowed
>    so 12 or more platforms fit (phase-542 W5).

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
the generated region, a `weekly.yml` matrix entry the registry does
not name, a `ROWS` entry with no row).

Generated views, with the hand-authored thing each replaces:

| View | Replaces | Shape |
| --- | --- | --- |
| `matrix::CELLS` rows per board | hand `cell(...)` lines in `matrix.rs` | a generated `include!` region; the witness rows come from the family's feature × language × RMW × edition table in the registry, inheriting rows get the smoke cells. Comments that carry history move into the row's `notes`. |
| `interop::CELLS` smoke rows | hand `ic(...)` lines | one row per inheriting board whose smoke needs a ROS 2 peer |
| emulator fixture rows in `examples/fixtures.toml` | hand `[[fixture]]` rows for emulator builds | the row names a coordinate and a locator port; build arguments come from the descriptor and the family defaults |
| `weekly.yml` matrix and the probe case table | the `matrix:` list and `run-bootstrap-probe.sh`'s `case` | one JSON file the workflow and the script both read |
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

## Amendment 2026-11-22: D13 — a board's status is what ran, collected automatically

> **Ruled 2026-11-22 (user, relayed by the nros 241 manager): all four
> decisions as recommended; Stable once merged.**
> [phase-585](../roadmap/phase-585-board-status-from-ci-outcomes.md) carries
> the work items.

**The goal (user):** no manual maintenance of board status. A test's board
is collected automatically from what the test already resolves, the status
follows from what ran, and the results are reported where people look: the
run's own summary, a results file, the book, and the CLI.

### The gap

D10 derives the status word from the registry's CLAIM and from lanes that
EXIST: `cadence = "daily"` needs a nightly job for the row, and
`check-board-registry` / `check-cadence-jobs` hold that. Nothing asks whether
the lane PASSED. On 2026-11-22 the nightly's `ThreadxLinux` job was red while
`board-support-levels.md` said ThreadX-on-Linux is "Verified on host
(daily)". Over the last six nightlies (newest first, per-board job
conclusion, read by hand): `ThreadxLinux` ✗✓✓✓✓✓, the Zephyr `ci-both` job
✗✓✓✓✓✓, `FreertosMps2` ✓✓✓✓✓✗, every other board ✓. Two boards, `Linux` and
`ZephyrNativeSim`, have no nightly job of their own: their cells ride other
lanes, so a board-to-JOB map cannot see them at all.

What already exists, and is reused:

- the fixture resolver attributes every artifact a test resolves to its
  manifest row and coordinate (`fixtures::lane::attribute_path`,
  `attribute_workspace_id`, over `fixtures-manifest.py`'s `row_coord()` /
  `row_artifact_root()`), for lane skipping; an interop test states its
  coordinates (`interop::assert_test_bound`);
- `scripts/ci/nightly-triage.py` classifies a failed job's failing STEP as a
  VERDICT or NO VERDICT ("a never-ran is not a fail");
- `scripts/release/record-lane-runs.py` (issue 2325) records each lane's
  newest verdict run into `board-support-runs.json` at release, which
  `nros board show` and `nros doctor --report` read offline.

### D13.1 — Evidence comes from what a test resolves, never from a list

A test's board is the registry row of the coordinate it resolves. When a
test resolves a fixture, the resolver already knows the row and coordinate;
it now also appends one line to the run's evidence ledger
(`$NROS_TEST_EVIDENCE_DIR`, one JSONL file per test process):

```json
{"test": "nros-tests::emulator::threadx_linux_pubsub", "row": "threadx-linux-c-zenoh-talker",
 "coord": {"platform": "threadx-linux", "lang": "c", "rmw": "zenoh", "kind": "pubsub"}}
```

The test's name is the harness's (the test thread's name; nextest runs one
test per process). Interop tests add their `assert_test_bound` coordinates
the same way. No test carries an annotation, and no list names tests.

After the run, the collector (D13.3) joins the ledger with the junit: each
test that resolved a coordinate yields one **evidence record** per
coordinate:

| Field | From |
| --- | --- |
| `board` | the registry row whose `matrix_platform` is the coordinate's platform |
| `cell` | the coordinate (platform, lang, rmw, kind) |
| `test` | the ledger |
| `verdict` | the junit: `passed`, `failed`, or `skipped:lane` (a lane skip is not evidence) |
| `run`, `workflow`, `event`, `sha`, `date` | the CI run (or the local tool, D13.5) |
| `source` | `ci` or `report` |

A coordinate whose platform has no registry row FAILS the collector, naming
the test and the coordinate. A board is evidenced by whichever lane ran its
cells, so `Linux` (host-tests, merge lanes) and `ZephyrNativeSim` (the Zephyr
jobs) are covered with no job of their own.

A job that never reached its tests leaves no records. The collector also
classifies every job by its failing step (nightly-triage's rule, moved into a
shared module), so a board whose cells were EXPECTED in that job (D13.4) and
are absent is recorded as `no-verdict` with the job's reason, not as
nothing.

### D13.2 — A board's outcome per run

| Outcome | Condition |
| --- | --- |
| `passed` | at least one record for the board, and none `failed` |
| `failed` | some record for the board `failed` (the failing cells are listed) |
| `no-verdict` | no records, and a job that would have run its cells failed in a NO VERDICT step or was cancelled |
| `not-run` | no records, and no job of the run would have run its cells |

### D13.3 — The results file, one per run

The machine-readable input to every view. A final `board-results` job in
`nightly.yml`, `weekly.yml`, `merge-runtime.yml` and `host-tests.yml`
(`needs:` every job, `if: always()`) runs `scripts/ci/board-results.py`: it
downloads the run's junit and evidence ledgers, reads the jobs and steps
(`gh run view --json jobs`), and uploads `board-results.json`:

```json
{
  "schema": 1, "source": "ci", "workflow": "nightly.yml", "event": "schedule",
  "run": 123, "url": "...", "sha": "...", "date": "2026-11-22",
  "boards": {
    "threadx-linux": {
      "outcome": "failed",
      "cells": [{"cell": "threadx-linux/c/zenoh/pubsub", "test": "...", "verdict": "failed"}],
      "job": "ThreadxLinux", "step": "Run tests", "class": "verdict"
    }
  }
}
```

Kept 90 days as an artifact. It is never committed to `main`.

### D13.4 — What the registry keeps, and what becomes observed

The registry keeps only what cannot be observed: what the board IS.

| Field (today) | After D13 | Why |
| --- | --- | --- |
| `crate`, names, `matrix_platform`, `board_keys`, `external`, `flavour` | **kept** | identity: which package, which platform token. Nothing runs to tell you this |
| `infra` | **kept** | a crate that is not a board |
| `breadth = "witness"` / `"inherits <p>"` | **kept** | a claim that two boards share a code path. No run can show that two code paths are the same |
| `emulator` | already **observed** from the descriptor (RFC-0118 D4) | unchanged |
| `runs` | **observed**: `host` / `emulator` from the runner the evidence used (the descriptor's `[board.run]`), `hardware` only from a `report` record made on hardware (D13.5) | the evidence says where it ran |
| `cadence` | **observed**: `merge` if a `merge_group` run produced passing evidence in the window, else `daily` if a scheduled nightly did, else `weekly` if a scheduled weekly did, else `manual` if a `report` record did, else none | the cadence is which scheduled lane actually delivered results |
| `last_run` | **observed**: the date of the newest `report` record | the hand-typed date is the manual maintenance this removes |
| breadth's cells | **observed**: the cells that PASSED in the window (D11 derives them from DECLARED cells today) | breadth is what ran, not what was declared |
| `roads` | **kept** as the expected lanes, checked against evidence | a road is how a board is reached; its evidence is its lane's records |

So the typed evidence fields (`runs`, `cadence`, `last_run`) leave the
registry, migrated by deleting them once the derivation reads evidence.
`check-cadence-jobs`' promise ("the registry may not promise a cadence the CI
does not run") becomes true by construction, since nothing promises.

The expected cells of a board (what the collector checks for absence) are
its declared cells in `matrix::CELLS` / `interop::CELLS`, which the generator
already derives; those stay the PLAN, and the records are the RESULT.

### D13.5 — The status word, from evidence

The word is derived from the board's records in a window (`outcome_window`, a
top-level registry value, three cadence periods to start), never typed:

| Status word | Condition |
| --- | --- |
| **Verified in emulator / on host / on hardware (cadence)** | the observed cadence's newest outcome is `passed` within the window |
| **Failing since <date>** | the newest outcome with a verdict is `failed`; the date is the first `failed` of that streak |
| **Not run since <date>** | no verdict in the window; the date is the last `passed` |
| **Boots, manual (date)** | the only evidence is a `report` record, newer than `manual_stale_days` |
| **Builds** / **Not supported** / **Infrastructure** | as D10: no runtime evidence, and a build cell or none |

A single red does not degrade the word: the window absorbs one bad night, and
the per-run views still show it. Maintainer counts (D10) key on the identity
rows, not on the moving word.

### D13.6 — Manual runs: `nros board report`

Hardware, FVP and PX4 SITL runs are made by a person. `nros board report
<board>` runs the board's declared cells locally (the same tests, the same
resolver, so the same ledger) and writes a `report`-source results file with
the host's facts (tool version, commit, runner: hardware probe or emulator).
`--submit` publishes it.

Where `report` records live, without committing to `main` nightly:

- **(A) An `evidence` branch (recommended).** An orphan branch of this repo,
  `board-evidence`, holding one file per report
  (`reports/<board>/<date>-<sha>.json`). Pushes never touch `main`, so the
  merge queue is unaffected, and the history is auditable. The book deploy
  and `record-lane-runs.py` read it.
- **(B) A release asset per report.** No branch, but assets are hard to list
  and to prune, and a report between releases needs a release to hang on.
- **(C) A separate data repository.** Clean, but a second repo to provision
  and to grant push to.

A `report` record is trusted as its submitter is, as the typed `last_run`
was; what changes is that it carries the cells, their verdicts and the commit,
so it is evidence rather than a date.

### D13.7 — Where people see it

1. **Per run: the job summary.** The `board-results` job writes a table into
   `$GITHUB_STEP_SUMMARY`: board, outcome, cells passed / failed, the failing
   cell and step, verdict or no-verdict. The run's page answers "which boards
   broke tonight" with no log.
2. **Per run: `board-results.json`** (D13.3), the input to everything below.
3. **The standing view: the book, rendered at deploy.** `publish.yml`'s
   `deploy` gains a `workflow_run` trigger on the nightly, the weekly and
   `host-tests`, collects the newest results (at most 30 runs per lane, plus
   the `board-evidence` branch), and an mdBook preprocessor fills generated
   marker regions through D13.5's derivation:
   - `support-status.md`, per board: the last N runs as a row of marks
     (`✓ ✓ ✗ ✓ ✓ ✓`, newest first, each a labelled link to its run; see the
     run marks below), a summary sentence, the date it last went red and why (verdict, with the failing cell, or "did not run: a CI
     problem"), the labelled claim, the cells that ran;
   - `supported-boards.md`, per row: the derived word and one "Last run:"
     line.
   The deploy also publishes the merged `board-status.json` at the site root.
   No CI output is committed (issues 0883/0884). A book built locally has no
   results and says so ("Last run: not recorded in this build of the book.").
4. **The CLI: `nros board list --status`** (and `show`, `nros doctor
   --report`) reads the same records: offline from the release's
   `board-support-runs.json` (issue 2325, grown into a results history), or
   with `--online` from the published `board-status.json`. A checkout with
   neither says "not recorded in this checkout". Nothing fetches
   implicitly.

The book, the CLI and the summary use ONE derivation (a Python module the
preprocessor and the collector import, with a Rust reader in the CLI) and
one fixture of histories and expected words that both run, so they cannot
disagree, and none of them reads a typed status.

**Book wording (agreed with the book's UX role, nxp 139 AUO book,
2026-11-22).** Every outcome line leads with "Last run:", absolute dates only,
no run ids on audience pages (at most a "(log)" link):

- "Last run: passed, 2026-11-22"
- "Last run: failed, 2026-11-22. Failing since 2026-11-20 (3 runs)."
- "Last run: did not run, 2026-11-22. A CI problem, not this board."
- "Last run: not recorded in this build of the book."

**The run marks (support-status), accessible by construction:**

- A word lead-in with the fixed N the generator names (six):
  "Last 6 runs, newest first: ✓ ✓ ✗ ✓ ✓ ✓". Without "newest first" the
  direction is unknown, and a fixed N means the row never silently changes
  length.
- A space between marks, so they can be counted.
- THREE marks, so a CI outage never reads as a board failure: ✓ passed,
  ✗ failed, – did not run.
- Each mark is a link whose label names it, as `title` and `aria-label`
  ("passed, 2026-11-22"; "failed, 2026-11-20"; "did not run, 2026-11-21: a CI
  problem, not this board"), so a screen reader reads dated outcomes, and the
  meaning never rests on glyph shape or colour alone.
- Then one summary sentence that needs no marks, which is what most readers
  and the page search use: "5 of the last 6 runs passed. Last failed
  2026-11-20: <why>."

The degraded word REPLACES the claim on an audience page (never struck
through); the claim stays on `support-status.md` as a labelled line,
"Claimed: Verified on host (daily)". Pages that must not carry a moving
outcome: `board-support-levels.md` (it defines the words and the outcome
lines), the starters and the Tutorial's board pick (a stable claim, linking
to the board's `supported-boards` row), and release notes (frozen at the cut,
or omitted).

### D13.8 — The gate: consistency both ways

Static, no network, on the fast line (`check-board-registry` and
`check-cadence-jobs` grow):

1. every fixture row's platform and every `matrix::CELLS` / `interop::CELLS`
   platform has a registry row (a result with no board cannot be produced);
2. every non-infra registry row has at least one declared Runtime or
   BuildOnly cell (a board with no possible result);
3. the resolver writes a ledger line on every path that attributes a row
   (a self-test: each attribution entry point, under a temp ledger, writes
   exactly one line);
4. the collector's self-test over recorded junit, ledgers and run JSON: a
   pass, a failed cell, a set-up failure, a cancelled job, a test whose
   coordinate has no row (refused), a board riding another board's job;
5. the outcome markers sit on exactly the pages D13.7 names.

At run time the collector refuses a coordinate with no row, and the standing
view reports a board with no records in the window as "Not run since", so a
board with no results is visible, not silent. Drift between the status and
the book cannot exist: the book has no typed status to drift from, it renders
from the same records.

### Options considered

- **Board outcome from JOB conclusions** (the first draft of this
  amendment): cheap, but it cannot see a board with no job of its own
  (`Linux`, `ZephyrNativeSim`) and it says nothing about which cell failed.
  Kept only as the no-verdict signal (D13.1).
- **A per-test annotation naming the board.** Rejected by the goal: it is a
  hand-maintained list in another place.
- **Keep `cadence` typed and check it against evidence.** Smaller change, but
  a person still edits it when a lane moves, which is the maintenance the
  goal removes.
- **Client-side rendering of a published JSON.** A saved or offline book then
  shows nothing, and the book has no JavaScript; not recommended.
- **Committing results.** Serialises the merge queue (issues 0883/0884).

### Decisions (ruled 2026-11-22, user)

1. D13.4: the typed `runs`, `cadence` and `last_run` are RETIRED and
   observed from evidence. Identity, `infra` and witness/inherits stay typed.
2. D13.5: the word degrades after three cadence periods with no `passed`,
   into "Failing since <date>" or "Not run since <date>"; a single red does
   not degrade it.
3. D13.6: `report` records go on the `board-evidence` orphan branch, through
   `nros board report --submit`.
4. D13.7: the book is rendered at deploy, with a published
   `board-status.json`, and N = 6 run marks.

## Evaluation and decision steps

Each decision is validated before the next one is relied on, and each has a
stated reversal.

1. **Round-trip first, change nothing.** The generator's first milestone is
   to regenerate today's `matrix.rs` cell region, `fixtures.toml` emulator
   rows, `weekly.yml` matrix, the probe case table and the 2169
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

Filed as [issue 2536](../issues/2536-board-registry-open-questions.md) (2026-11-14).
All three were ruled on 2026-11-17 (decision log). The questions as filed:

- Whether `nightly_token` stays an authored field or is derived from the
  board's `PlatformId::just_module`. Phase-558 made the nightly verdict
  per board (one job per `matrix_platform`), so the token now names only the
  module whose verbs a board's job runs; `lane-coords --scope <module> --board
  <PlatformId>` refuses a token and board that disagree.
- Whether the per-board evidence page should also carry the measured
  numbers RFC-0115 asks for (time to first output on that board, install
  minutes), or link to where the probe records them.
- How a downstream board package (RFC-0064's out-of-tree BSP) publishes its
  own registry row so `nros board list` shows the same status words for a
  board nano-ros does not ship.

The rulings:

- **`nightly_token` is derived** from `PlatformId::just_module` of the row's
  `matrix_platform`. Measured: all 9 of the 10 token rows that name a
  `matrix_platform` already equal the derived value. The field stays
  authored only on a row with no `matrix_platform` (today one row,
  `nros-board-mps3-an536-freertos`). The work is phase-558 W8.
- **The evidence page links to the probe's numbers once the probe records
  them,** and shows no number until then. It never shows a hand-copied
  number, because that is the drift this registry exists to stop. Open until
  the probe records time to first output and install minutes per board.
- **A downstream board package carries its own registry row,** and its status
  can claim no more than its own CI proves. The shape is decided now. The
  merge into `nros board list`, which today reads only this tree's
  `board-support.toml`, is built when the first out-of-tree board package
  wants a row.

## Decision log

- 2026-11-22 — D13 drafted (design only, phase-585), for the user's
  decisions listed in the amendment. Reworked the same day for two added
  user requirements: no manual maintenance (evidence collected from what a
  test resolves; cadence, runs, last_run and breadth observed), and the
  reporting surfaces as a deliverable (job summary, results file, book,
  CLI). The user ruled all four decisions as recommended (retire the typed
  evidence fields; degrade after three cadence periods; `board-evidence`
  branch; deploy-time render with a published JSON, N = 6).

- 2026-11-17 — the three open questions (issue 2536), ruled by the user,
  accepting the board-support decision brief as written:
  (1) `nightly_token` is derived, and stays authored only where no
  `matrix_platform` exists (phase-558 W8);
  (2) the evidence page links to the probe's numbers once the probe records
  them, and shows none before then;
  (3) a downstream package carries its own row, capped at what its own CI
  proves, and the merge is built when the first one arrives.

- 2026-11-07 — phase-542 W0, three user rulings for PX4 SITL's row (PR #3169):
  (1) "Boots, manual (date)" widens to `runs = "host"`; no new status word,
  so `status_word` needs no change (it already read so) and only D10's table
  moves. (2) An `external` row with a `matrix_platform` and no board crate
  states its `flavour` (`std` or `no_std`) in a new field, which
  `check-flavour-lanes` reads in place of a crate; the gate still checks the
  row rather than excluding external rows, and `shape_errors` refuses the
  field anywhere else. (3) `cadence = "daily"` for PX4 needs a real Runtime
  cell, so the DDS domain window (`alloc::domain_of`, 21 wide, 11 platforms;
  `Px4` is index 12) is narrowed so 12 or more platforms fit. That is
  phase-542 W5's, and until it lands the row stays `cadence = "manual"`.
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
- 2026-11-13 — D10 gains the optional `roads` field (phase-550 W13): generic
  rather than a NuttX special case, absent meaning one road on the cadence's
  lane, and each lane checked in both directions.

## Review (2026-11-13, phase-555 W2)

**Verdict: keep.** The status question is brief item P4.

- Checked against the code:
  - `packages/boards/board-support.toml` carries `runs` on 17 rows and
    `tier` on none, with `manual_stale_days = 120`.
  - The generator `scripts/gen-board-support.py`, the cell table
    `packages/boards/board-support-cells.toml` and the gate
    `check board-registry` (`just/check/platform.just`) exist.
  - So do the generated `book/src/reference/board-support-levels.md` and
    the book-time staleness preprocessor `scripts/mdbook-board-status.py`.
  - The retired `board-support-tiers.md` is gone. phase-522 is archived as
    complete (PRs 2932, 2937, 2941, 2942).
- Evaluation step 1's narrowing (fixture rows checked, not generated) is
  already recorded in the decision log. The code matches that narrowing,
  not D12's original table.
- Stale references: the migration table lists `nros-board-esp32-qemu`,
  which was retired after it was written (RFC-0113 D5). The table is
  history, so it is left as written. The registry has no esp32 row.
- Pairs:
  - With RFC-0115 and RFC-0118: both amend D10 consistently, and the
    `[board.run]` move is recorded here.
  - RFC-0123 and RFC-0124 cite D10 and D12 as authoritative: no conflict.
  - RFC-0051: D12's reversal clause was partly exercised, and RFC-0051's
    authored fixture shape stays for fixture rows, as the log says.
  - RFC-0112: its tier numbers are the one place this RFC's vocabulary is
    not yet used (brief P6).
- `area:` platform-board-rtos is right.
- Status: Draft, though phase-522 shipped it. Brief item P4.

**W3 (2026-11-14).** P4 (c) under A1: the open questions filed as issue 2536 and pointed at; then Stable. ARCHITECTURE §4 updated.
