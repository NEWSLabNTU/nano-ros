# Phase 342 — Test runtime reduction: the consumer side

**Status (2026-08-08). W1–W3 LANDED; W4 blocked (structurally); W5 retracted on
measurement; W6 answered.** The measurable half is done:

| item | before | after | |
| --- | --- | --- | --- |
| W1 `native_example_pubsub` | 95.1 s (1 test) | 34.1 s (10) | 2.8× |
| W1 `native_example_reqresp` | 82.8 s (1) | 15.1 s (19) | 5.5× |
| W1 `workspace_features` | 62.9 s (1) | 18.2 s (18) | 3.5× |
| W2 `native_main_macro_misuse` | 108.5 s | 10.3 s warm | 10.5× |

**~350 s of tier-1 test time → ~78 s**, and the lane's wall-clock FLOOR — the
longest single test — moves from 95.1 s to 34.1 s. Coverage identical
throughout; every `#[case]` list is bound to `matrix::CELLS` by a tripwire that
was verified to FAIL before being trusted.

W3 gated the lane arithmetic that four sites carried by hand and three had got
wrong. W4 is parked with its blocker named. W5 is retracted rather than
attempted — the measurement said the rows it wanted to delete are load-bearing,
which is the same answer phase-329 W8 got. W6 is answered below.

**Informed by:** RFC-0061 (the tier ladder and its cost unit), RFC-0051 (the cell
tables), RFC-0066 (example/fixture consolidation), phase-329 (whose W8 verdict
sets this phase's non-goals), phase-340 + phase-334 (the build-side program this
phase deliberately does not duplicate).

## The reframe this phase starts from

"Make the tests faster" is the wrong target, and the measurement that says so is
already in the tree. phase-318 timed tier 1 on one machine
(`archived/phase-318-fixture-freshness-and-tiers.md:238-249`):

| stage | cold (post-pull) | warm |
| --- | --- | --- |
| `check-fast` + feature checks | 409 s | 69–82 s |
| `check` complete | 1664 s | ~600 s |
| + `rust-rtos-link-check` | 2675 s | ~600 s |
| full lane | **3157 s (52 m)** | **1215 s (20 m)** |

with the verdict: *"the test execution is 226 s"* — 7 % of the cold lane. **`check`
is the tier's cost, and cache state dominates it.**

That is why this phase is scoped to the CONSUMER side and not to compilation.
Compilation belongs to phase-340 (WHAT gets compiled, how often) and phase-334
(WHERE the cache lives). Duplicating them is how 334 and 340 ended up
re-deriving each other's findings — recorded at `phase-334:13-16`. **This phase
owns a third axis: how much work a lane ASKS for, and how the tests consume what
it built.**

## Measurement — tier 1 test execution, 2026-08-07

Full `just ci` run in the ROS distrobox, native lane rebuilt immediately before.
Parsed from the run log (1259 tests):

```
summed test time   1476 s
wall               387 s        => ~3.8x effective parallelism
```

Distribution — the reason this phase targets tens of tests, not twelve hundred:

| bucket | tests | summed | share |
| --- | --- | --- | --- |
| < 0.1 s | 1006 (80 %) | 8 s | **0.5 %** |
| 0.1–1 s | 92 | 40 s | 2.7 % |
| 1–5 s | 87 | 217 s | 14.7 % |
| 5–20 s | 58 | 512 s | 34.7 % |
| **> 20 s** | **16 (1.3 %)** | **699 s** | **47.4 %** |

Top binaries by summed time:

| binary | summed | tests | s/test |
| --- | --- | --- | --- |
| `native_main_macro_misuse` | 199.4 s | 5 | 39.9 |
| `native_api` | 102.0 s | 32 | 3.2 |
| `native_example_pubsub_e2e` | **95.1 s** | **1** | 95.1 |
| `multi_node` | 86.5 s | 8 | 10.8 |
| `native_example_reqresp_e2e` | **82.8 s** | **1** | 82.8 |
| `xrce_ros2_interop` | 71.7 s | 9 | 8.0 |
| `zero_copy` | 63.7 s | 3 | 21.2 |
| `workspace_features_e2e` | **62.9 s** | **1** | 62.9 |
| `large_msg` | 58.9 s | 10 | 5.9 |
| `nano2nano` | 47.7 s | 10 | 4.8 |

**The floor is a single test.** Wall time can never fall below the longest test,
and that is `native_example_pubsub` at 95.1 s — one `#[test]` that loops over 18
`matrix::CELLS` internally (`native_example_pubsub_e2e.rs:106-126`). No amount of
nextest parallelism enters a test body.

## Work items

### W1 — Split the serial cell-loop tests so nextest can schedule them

`native_example_pubsub` (95 s / 18 cells), `native_example_reqresp` (83 s / 18)
and `workspace_features` (63 s) are each ONE test folding over a derived cell
list. They serialize by construction, and together they are 241 s of the 1476 s
summed — but more importantly they set the wall-clock floor at 95 s.

Emit one test per cell (the tree already does this elsewhere — `entry_matrix`
uses `rstest` case generation, and `interop_e2e` is parametrized per cell), so
the 18 cells schedule against the 3.8× parallelism the lane already has.

Second, independent benefit: **failure attribution**. Today a failure reads
`native_example_reqresp: 1 of 18 cell(s) FAILED` and the whole test is red; the
other 17 cells' verdicts are lost. Per-cell tests name the failing coordinate,
which is what #0422's triage had to reconstruct by hand.

*Acceptance:* the three binaries report ≥18 tests each; tier-1 wall drops and the
new floor is measured and recorded here.

*Risk:* these cells share per-RMW routers/agents (`native_example_pubsub_e2e.rs:172`
keeps a guard alive per cell). Splitting must keep that isolation — the likely
shape is a nextest test-group, not a shared static.

#### W1 RESULT (2026-08-08) — landed, and it changes a rule

| consumer | before | after | |
| --- | --- | --- | --- |
| `native_example_pubsub` | 1 test, 95.1 s | 10 tests, 34.1 s | 2.8× |
| `native_example_reqresp` | 1 test, 82.8 s | 19 tests, 15.1 s | 5.5× |
| `workspace_features` | 1 test, 62.9 s | 18 tests, 18.2 s | 3.5× |

241 s of serial critical path → 67 s; the tier's floor moves from 95.1 s to
34.1 s. Coverage identical — same cells, assertions and isolation — each
`#[case]` list bound to `matrix::CELLS` by a tripwire **verified to fail on
drift** before being trusted.

**The lesson is not about speed.** `workspace_features` did not get faster when
split: 58.8 s for 18 tests whose slowest cell is 5.7 s. The cause was already
written in `.config/nextest.toml` — the `and test(qos)` filter had been DROPPED
because the fold left no per-cell test to match, so all 17 cells joined a
`max-threads = 1` group that only THREE need (issue 0312, discovery contention).
Restoring per-cell tests made the narrow filter expressible again: 58.8 → 18.2 s
with no change to what is serialized.

> **A fold does not just serialize its own cells — it erases the names that
> everything else needs to talk about them:** schedulers, `-E` filters, timeout
> budgets, test groups, and failure reports. The serialization is recoverable;
> the lost vocabulary is what made it invisible.

**Consequence for consolidation work.** All three folds came from the phase-329
program (W4 for the two example consumers, W1 for `workspace_features`), whose
goal was fewer test FILES. That goal is fine; the shape it used is not. Phase-329
is complete and lists two deferred folds — `xrce.rs` and
`emulator.rs`/`esp32_emulator.rs`. Those files hold 6, 16 and 8 tests today.

**If they are ever consolidated, consolidate the FILE and keep one test per
cell** (`#[rstest]` + `#[case]`, as W1 leaves all three consumers). A single-test
fold would re-create exactly the cost measured here, and the file-count metric
would not notice.

### W2 — Stop paying five cold `cargo check`s in `native_main_macro_misuse`

199 s, 13 % of all test time, 5 tests. These compile at run time as a
**documented exception** (`native_main_macro_misuse.rs:3-15`) and the reasoning
is sound: a build that must FAIL cannot be prebuilt, and the rebuild case needs
two checks across a file touch. The exception is not the problem — the cost is:
its own comment records that "a cold check exceeds the 60s default".

Five cases each pay a cold check against a staged copy of the same template. Give
them one shared, warm `CARGO_TARGET_DIR` (per-binary, not per-case), so only the
first pays cold.

*Acceptance:* summed time for that binary measured before/after; the exception
and all five assertions unchanged.

*Care:* the rebuild-tracking case asserts a re-check HAPPENS after a touch. A
shared warm dir must not mask it — that case may need to keep its own dir, which
still leaves four sharing.

### W3 — Make the lane arithmetic say the same number in all four places

`lane-coords` is the authority, and three call sites have drifted from it:

| source | says | live |
| --- | --- | --- |
| `justfile:2207` | tier 2 = "12 of 47" | 13 coords / 12 cells |
| `justfile:2245` | tier 2n = "33 of 47" | 35 coords / 35 cells |
| `ci_lane.rs:153` | "11 of 46 coordinates" | 13 of 47 |

RFC-0061 already had to amend itself once for exactly this class — it quoted
tier 2 at "~20 % of a full sweep" counting CELLS when the cost unit is
COORDINATES, where the same cover is 70 % (`0061:211-217`). A stale spelling is
how a tier gets chosen on a wrong cost estimate.

*Acceptance:* the three sites derive from `lane-coords` or are gated against it;
no hand-written count survives.

### W4 — BLOCKED, and structurally so (verdict 2026-08-08)

Not attempted. The justfile states the dependency and it has not moved:

> Narrowing the build here would need the run narrowed to match first; until
> then, saying so beats a lane that silently under-builds.

Narrowing the RUN is phase-329 W8.d, which is blocked on "every test
cell-bound". That did not become true when phase-329 closed: its W4 was a
DISPOSITION pass, and it concluded the opposite — ~36 of the candidate files are
genuine one-offs (behaviour/boot/QoS/error/edge tests no matrix cell covers).
A test with no coordinate cannot be selected by a coordinate-scoped run, so
scoping the run today would silently drop them.

Reopen only when tests without a cell are either bound to one or explicitly
exempted. Until then this item is correctly parked, and the lane is honest about
building more than it gates.

### W5 — RETRACTED, the premise does not survive measurement (2026-08-08)

W5 proposed collapsing the native example variants on RFC-0066's figures. Those
figures are real — 47 dirs are built more than once, accounting for 119 of 240
`[[fixture]]` rows — but they do not mean what the work item assumed. The four
`examples/native/rust/talker` rows are:

```
(bare)                     compile-ASSERT fixture, no rmw, skipped by coord lanes
target-tls                 the TLS variant
rmw=zenoh  -> target-zenoh runtime fixture for the zenoh cells
rmw=xrce   -> target-xrce  runtime fixture for the xrce cells
```

which is precisely what phase-329 W8.a RETRACTED after testing it: *"the
`target-<rmw>/` dirs are NOT redundant duplicates; they are the RUNTIME fixture
locations the binary-locator requires"* (`fixtures/binaries/mod.rs:718-724` maps
`Rmw::Zenoh -> "target-zenoh"`). Deleting a twin breaks binary resolution.

The "34 of 42 rows declare no `features`/`cmake_defs`/`env`" statistic does not
identify redundancy either: for these rows the variance IS the `rmw` and
`target_dir`, so a metric that only looks at feature keys reports every runtime
fixture as variance-free.

**This item should not have been written.** This phase's own non-goals cite
329 W8's verdict — "the fixture-BUILD burden is NOT reducible by deleting rows" —
and W5 then proposed an exception on the strength of a row count, which is the
unit that verdict rejects. Left in place, retracted and explained, so the next
reader inherits the measurement rather than the idea.

### W6 — What each tier-1 board witnesses (2026-08-08)

Runtime cells and the `(lang, rmw)` pairs each tier-1 board covers, computed
from `matrix::CELLS`:

| board | runtime cells | (lang,rmw) pairs | pairs unique to it |
| --- | --- | --- | --- |
| `linux` | 72 | 10 | none |
| `zephyr` (native_sim) | 39 | 10 | none |
| `threadx-linux` | 18 | 6 | none |
| `mps2-an385-freertos` | 15 | 4 | none |
| `nuttx-qemu` (arm) | 14 | 3 | none |

**No tier-1 board contributes a `(lang, rmw)` pair the others lack.** That is not
an argument for cutting one: it says their value is entirely on the PLATFORM
axis — toolchain, libc, linker, image ownership — which is exactly the axis
RFC-0061 assigns platform ("selects toolchain + libc + linker; pairwise with
lang") and where the 0268 / 0245 / 0332 defect class lives. Judge a tier-1 board
by the platform property it witnesses, never by its cell count.

So, what each is the witness FOR:

- **`linux`** — the functional surface. All three RMWs, all languages, and the
  only board where a failure is cheap to debug. 72 of 176 runtime cells.
- **`zephyr` (native_sim)** — the Zephyr API surface, and **only** that. All 28
  Zephyr runtime configs target `native_sim/native/64` (`0064:585-596`), so every
  Zephyr test bypasses Zephyr's own IP stack. Ten coordinates — tied with linux
  for the most — witnessing one board that is not the interesting one. This is
  the weakest coverage-per-coordinate in the tier, and phase-337 W2 added the
  Cortex-M witness at tier 2 precisely because of it.
- **`threadx-linux`** — the NSOS shim: ThreadX's API over a host libc, so the
  RTOS abstraction is exercised without an emulator in the loop.
- **`mps2-an385-freertos`** — ARMv7-M with a nano-ros-OWNED image (we place the
  linker script and the boot path). Also the most expensive build in the tier at
  ~1370 s (`0064:790-791`).
- **`nuttx-qemu` (arm)** — ARMv7-A where the KERNEL owns the build: NuttX's
  Kconfig/Make.defs drive it and nano-ros is an app. The complement to the
  FreeRTOS row, and the reason both are in tier 1.

### W4 (original text) — Tier 2 builds `all`; scope it to its coordinates

`justfile:2222-2227` states it plainly: tier 2's saving today is in the staleness
GATE, not the build — it still builds every row. Measured: tier 2 needs **89 of
333** manifest rows.

This is the largest single lever in the ladder, and it is **blocked on
phase-329 W4** (fewer boots) per that phase's W8.d. Recorded here so the
dependency is visible; not startable in this phase alone.

### W5 — Collapse the native example variants

RFC-0066 measured it (`0066:60-67`): the build cost is native, not exotic —
**37 native example directories are built 2–8 times as variants, accounting for
120 of 180 native rows**, and **34 of the 42 themed-workspace rows declare no
`features` / `cmake_defs` / `env` at all**. Linux is 190 of 333 rows (57 %).

This is the one place row-count reduction is defensible, because RFC-0066 already
identified rows that differ in nothing.

### W6 — Say what tier 1 actually witnesses

Tier 1's five boards are `linux`, `zephyr` (native_sim), `mps2-an385-freertos`,
`nuttx-qemu` (arm), `threadx-linux`. Two facts worth stating together:

- **All 28 Zephyr runtime configs target `native_sim/native/64`**
  (`0064:585-596`) — 10 of 47 coordinates, tied for the largest share, all
  witnessing ONE board, and every one bypasses Zephyr's own IP stack.
- Build wall clock is dominated by **FreeRTOS (~1370 s) and native (~1300 s)**
  per lane (`0064:790-791`).

So the platform with the most coordinates is not the one that costs the most, and
the coverage it buys is narrower than the count suggests. Deliverable is a
statement of what each tier-1 board is the witness FOR, so a future cut is made
on coverage rather than on count.

## Non-goals, each with the evidence that closed it

- **Deleting fixture rows.** phase-329 W8 tried it and every candidate was
  load-bearing: `target-<rmw>/` dirs are the runtime binary locator, edition
  workloads carry distinct RIHS01 hashes, robot1/2 are talker/listener halves.
  Verdict: *"The fixture-BUILD burden is NOT reducible by deleting rows — it is
  structural"* (`archived/phase-329:440-451`). W5 above is the exception it
  names, and it comes from RFC-0066's measurement, not from row-counting.
- **Merging board crates to save CI time.** *"merging crates removes no fixture
  rows, so it buys no CI time"* (`0064:789-793`). phase-322 was closed on
  2026-08-08 for the adjacent reason.
- **Filtering the test RUN per platform.** *"every cover touches all ten
  platforms by construction"*, so a nextest platform filter excludes nothing; the
  saving is entirely in which FIXTURES get built (`0061:304-308`).
- **Anything about compilation identity, target-dir layout, or caching.** Owned
  by phase-340 and phase-334 under their 2026-08-07 axis split.

## Acceptance for the phase

1. Tier-1 wall time re-measured on the same machine and lane, before and after
   W1+W2, with the distribution table above regenerated.
2. The wall-clock floor (longest single test) named and reduced.
3. No coverage lost: cell counts per platform unchanged, and `matrix_fixture_coverage`
   gates still green.
4. Every number in this document re-derived at close-out — RFC-0064's row figures
   were stale twice over within a week, and this phase's own numbers will age the
   same way.

## Opening measurements — provenance

The distribution and per-binary tables are parsed from a complete `just ci` run
in the ROS distrobox mirror on 2026-08-07 (1259 tests, `failures=10`, the run
recorded in issue 0422). The native lane was rebuilt immediately before, per that
issue's freshness rule. They are ONE machine and ONE run: treat them as the shape
of the problem, not as a benchmark.
