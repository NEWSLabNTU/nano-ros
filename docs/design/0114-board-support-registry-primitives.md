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

## Amendment 2026-11-22 (Draft): D13 — a status reports what the lane SAID, not only that it exists

> **Draft, for user decisions** (relayed by the nros 241 manager). Design
> only; [phase-585](../roadmap/phase-585-board-status-from-ci-outcomes.md)
> carries the work items once the questions below are ruled.

### The gap

D10's status word is derived from the registry's claim and from lanes that
EXIST: `cadence = "daily"` needs a nightly job for the row, and
`check-board-registry` / `check-cadence-jobs` hold that. Nothing asks whether
the job PASSES. On 2026-11-22 the nightly's `ThreadxLinux` job was red while
`board-support-levels.md` said ThreadX-on-Linux is "Verified on host
(daily)"; 2026-11-18 and 2026-11-19 had other reds. The word was true of the
lane and false of the board.

What already exists, and is reused rather than replaced:

- every automated cadence runs ONE job per registry row: the nightly per
  `matrix_platform` (phase-558, `scripts/ci/nightly-boards.py`), the Zephyr
  rows' 05:00 jobs, `merge-runtime` per `matrix.board`, and the weekly per
  `[[probe]]` track, each of which names its `row`;
- `scripts/ci/nightly-triage.py` classifies a failed job by its failing STEP
  into a VERDICT (build, test, e2e, ...) or NO VERDICT (set-up, provisioning,
  a jobserver stall), with a self-test, and issue 1754's step markers are
  shared with `lane-stage.py`;
- `scripts/release/record-lane-runs.py` (issue 2325) records, when a release
  is cut, each lane's newest run that reached a verdict with its per-job
  conclusions, into `packages/boards/board-support-runs.json`, which
  `nros board show` and `nros doctor --report` read offline.

So the missing parts are an outcome per ROW per RUN, a history of them, a
status that reads them, and a book that can show them without committing CI
output.

### Q1 — What counts as a row's outcome

**Recommended:** a row's outcome for one run of its lane is one of four,
decided from that row's job(s) by the triage classifier, never typed:

| Outcome | Condition |
| --- | --- |
| `passed` | every job of the row in the run concluded `success` |
| `failed` | some job of the row failed in a VERDICT step |
| `no-verdict` | the row's jobs failed only in NO VERDICT steps, or were cancelled or timed out before a verdict step ran |
| `not-run` | the run has no job for the row (skipped, filtered, or the workflow did not run) |

A row's jobs are its cadence's job plus every `roads` lane (D10's 2026-11-13
amendment), because a road is evidence of reach and its failure is the
board's. `no-verdict` and `not-run` are never a failure: they mean the
evidence is missing, which is a different statement, and the status says so
(Q3). This is `just nightly-triage`'s rule ("a never-ran is not a fail")
applied per row.

*Alternative considered:* per CELL outcomes from the junit (each test's
coordinate). Finer, and it would say "service failed, pubsub passed", but a
junit row maps to a coordinate, not a board, through `row_coord()`, and the
`[SKIPPED:lane]` rewrite makes junit counts lane-dependent. Recorded as a
later refinement: the record has room for a `cells` list, the status reads
only the row outcome.

### Q2 — How CI records it

**Recommended: one collector job per workflow run, reading the run through
the API.** `nightly.yml`, `weekly.yml` and `merge-runtime.yml` gain a final
job, `needs:` every board job, `if: always()`, that runs
`scripts/ci/board-results.py`. It reads the run's jobs and steps (`gh run
view --json jobs`), maps each job to its row by the same derivation that
named it (nightly-boards.py, the probe tracks' `row`, the merge board list),
classifies each with the triage classifier, and uploads one artifact,
`board-results.json`:

```json
{
  "schema": 1, "workflow": "nightly.yml", "run": 123, "url": "...",
  "event": "schedule", "sha": "...", "date": "2026-11-22",
  "rows": {
    "threadx-linux@ThreadxLinux": {
      "outcome": "failed", "jobs": ["ThreadxLinux"],
      "step": "Run tests", "class": "verdict"
    }
  }
}
```

Keyed by D12's `row_name`, the key `board-support-status.json` already uses.
A job the collector cannot map to a row FAILS the collector, naming it (so a
new job cannot report into nothing); a row with no job in the run is
`not-run`.

*Alternative considered:* each board job writes its own record in an
`if: always()` step from `${{ toJSON(steps) }}`. It needs an `id` on every
step and an edit to every job, and a job that never started writes nothing,
which is exactly the `no-verdict` case it must report. The collector sees
every job, started or not, from one place.

### Q3 — How the status word uses it

The claim stays where it is: the registry row, and the D10 word derived from
it ("Verified on host (daily)"). The outcome is a second, derived input, read
from the newest results; no file a person edits holds an outcome. Two
options:

- **(a) A qualifier only.** The word is unchanged and every view adds "last
  run: failed, 2026-11-22 (failing since 2026-11-20, 3 runs)". Honest and
  cheap, but a reader scanning the levels table still reads "Verified".
- **(b) The word degrades, like a stale manual row (recommended).** D10
  already degrades a manual row past `manual_stale_days` to **Builds** in
  every view, without editing the row. The same rule for automated rows:
  - **Verified … (cadence)** needs a `passed` within the row's freshness
    window: `outcome_window` cadence periods (a top-level registry value,
    3 to start: three nightlies, three weeklies);
  - otherwise, if the newest verdict is `failed`, the word is **Failing
    since <date>**, the date of the first `failed` in the current streak of
    verdict runs;
  - otherwise (only `no-verdict` / `not-run` since the last `passed`), the
    word is **Not run since <date>**, the date of that last `passed`.
  On an audience page the degraded word REPLACES the claim (never struck
  through: struck text is noise to a skimming reader and to a screen reader).
  The claim stays on the board's support-status evidence section as a
  labelled line, "Claimed: Verified on host (daily)", so a maintainer sees
  what the row promises beside what it delivered.

A single red does not degrade the word under (b): the window absorbs one bad
night, and the outcome line still shows it. Maintainer counts (D10) keep
keying on the CLAIMED word, because who looks after a board does not change
when its lane goes red.

### Q4 — How the book shows it without committing CI output

Committed files touched nightly serialise the merge queue (issues 0883,
0884), so the outcome must not be written into `main`. Options:

- **(A) Commit results nightly.** Rejected for that reason.
- **(B) Deploy-time render (recommended).** The committed pages carry the
  claim and a marker region, as the generator already writes. The book's
  `deploy` job (`publish.yml`, to `NEWSLabNTU.github.io/nano-ros-book`) gains
  a `workflow_run` trigger on the nightly and the weekly, downloads the newest
  `board-results.json` of each lane (and enough history for "since <date>",
  see Q2's retention below), and an mdBook preprocessor fills the regions
  through the same derivation as Q3. The deploy also publishes the merged
  results as `board-status.json` at the site root, the machine-readable feed.
  A book built locally with no results fills each region with "Last run: not
  recorded in this build of the book.", never with the claim alone.
- **(C) A published JSON plus client-side script.** The page fetches the
  JSON in the browser. A saved or offline copy then shows nothing, and the
  book has no JavaScript today. Not recommended.
- **(D) The release asset only.** Correct but stale between releases. It is
  kept for the CLI (Q5), not for the book.

**Book wording (agreed with the book's UX role, nxp 139 AUO book,
2026-11-22).** Every outcome line has one lead, "Last run:", with absolute
dates only and no run ids on audience pages (at most a "(log)" link; run
details belong to the evidence section):

- "Last run: passed, 2026-11-22"
- "Last run: failed, 2026-11-22. Failing since 2026-11-20 (3 runs)."
- "Last run: did not run, 2026-11-22. A CI problem, not this board."
  ("CI infrastructure" is kept for the evidence section.)
- "Last run: not recorded in this build of the book."

Where the moving outcome goes, and where it must not:

| Page | Outcome |
| --- | --- |
| `supported-boards.md` | one outcome line per row, and the degraded word in place of the claim |
| `support-status.md` (the evidence section) | the full history, the labelled claim, run details |
| `board-support-levels.md` | NONE: it defines the words, so it defines the outcome lines and the degraded words, and shows no board's outcome |
| starters, the Tutorial's board pick | NONE: a stable claim, linking to the board's `supported-boards` row for the live outcome (probed pages must not change per night) |
| release notes | frozen at the cut ("Last run before release: passed, …"), or omitted |

History for "since <date>": each run's artifact is kept 90 days, and the
render reads the lane's newest runs until it finds the streak's start
(bounded, e.g. 30 runs). An artifact is input to the render, never a file in
the tree.

### Q5 — `nros board list` / `nros doctor --report`

Offline by default, as now. `board-support-runs.json` (issue 2325, written at
release) grows into the same per-row outcome record, with the history the
status needs, and the CLI applies Q3's derivation to it with today's date
(as it already applies `stale_days`). A checkout has no such file and says
"outcome not recorded in this checkout". An explicit `--online` reads the
published `board-status.json`; nothing fetches implicitly.

The derivation has one implementation: the generator emits the thresholds
and the CLI and the preprocessor read them, and a shared self-test fixture
(a results history and the expected word) is run by both the Python and the
Rust reader, so the two cannot drift.

### Q6 — The gate that holds registry, CI and book consistent

`check-board-registry` and `check-cadence-jobs` grow, and stay static (no
network):

1. every row with an automated cadence has at least one job the collector
   maps to it, in the workflow its cadence names;
2. every job of `nightly.yml`, `weekly.yml` and `merge-runtime.yml` that runs
   board work maps to exactly one row (a job with no row is refused, naming
   it);
3. every fixture row's `platform` in `examples/fixtures.toml` and every
   `matrix::CELLS` platform has a registry row (a lane with no row is
   refused);
4. every generated status region on `supported-boards.md` and
   `support-status.md` carries the outcome marker the preprocessor fills, so
   neither can show the claim alone; and no starter, Tutorial,
   `board-support-levels.md` or release-notes region carries one (Q4's table);
5. the collector's self-test: recorded `gh run view` JSON (a pass, a verdict
   fail, a set-up fail, a cancelled job, an unmapped job) gives the expected
   `board-results.json`.

At run time the collector itself refuses an unmapped job (Q2), which closes
the direction a static gate cannot see (a job added by a matrix expression).

### What this does not change

The registry's primitives (D10's fields) and its cadence-to-job binding
(RFC-0122 D8/D9) are unchanged; RFC-0064's levels and RFC-0115's
emulator-first policy are untouched. No outcome is ever typed into the
registry.

### Decisions for the user

1. Q3: (a) qualifier only, or (b) degrade the word after the window
   (recommended), and the window (3 cadence periods proposed).
2. Q4: (B) deploy-time render with a published JSON (recommended).
3. Q1: row outcomes now, per-cell outcomes later (recommended), or per cell
   from the start.
4. Whether `no-verdict` runs, past the window, should read "Not run since"
   (recommended) or keep the claimed word.

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
  decisions listed in the amendment.

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
