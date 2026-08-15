# Phase 342 — Test runtime reduction: the consumer side

**Status (2026-08-10). COMPLETE. W1, W2, W3, W7, W8, W8b, W9 LANDED; W4
delivered by phase-340 W3 (it was parked here as blocked, wrongly — see W4);
W5 retracted on measurement; W6 answered.**

Measured, same machine, same lane:

| item | before | after | |
| --- | --- | --- | --- |
| W1 three cell-loop folds | 241 s | 67 s | 3.6× |
| W2 macro-misuse cold checks | 108.5 s | 10.3 s warm | 10.5× |
| W7 `rust_cyclone` readiness marker | 34.1 s | 4.0 s | 8.5× |
| W8 `native_api` sleeps | 102.0 s | 25.2 s | 4.0× |
| W8 `nano2nano` sleeps | 47.7 s | 8.8 s | 5.4× |
| W8b `emulator` settle sleeps | 24 s, 16 skipping | 16/16 run | issue 0483 |
| W8b `threadx_riscv64_qemu` | 7.37 s | 1.505 s | 4.9× |

Plus ~140 s of timeouts that were being waited out in SILENCE (issue 0481), and
the tier's wall-clock floor down from 95.1 s to ~34 s. Three gates now enforce
what had been convention: lane arithmetic, readiness literals, example output
conformance.

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
wrong. W4 was parked with its blocker named, and the block turned out not to
exist: phase-340 W3 had already built the escape the blocker named, the same day
it was re-confirmed. W5 is retracted rather than attempted — the measurement said the rows it wanted to delete are load-bearing,
which is the same answer phase-329 W8 got. W6 is answered below.

**What this phase turned out to be about.** It was opened to make tier 1
affordable, and it did that — ~350 s to ~78 s, the wall-clock floor from 95.1 s
to 34.1 s. But four of its results were not speedups at all. W7 found six
readiness greps waiting on markers their process never prints (issue 0481).
W8b found sixteen emulator tests that had been skipping in silence (issue 0483).
W8b again found a 2 s sleep that only Rust images paid, making one language 3.9×
slower than its siblings on a shared stack (issue 0484). W4 found a blocker that
had already been dissolved.

Each of those was uncovered by deleting a fixed delay, and none of them was
visible while the delay was there. **A sleep long enough to cover the worst case
is long enough to hide every case** — that is the sentence this phase bought, and
it is worth more than the seconds.

**W7 is not a performance item either.** Chasing W1's
`rust_cyclone` outlier (34.1 s against a 5.2 s sibling) ended at a one-word
disagreement between three implementations of the same standard ROS demo. Nine
call sites had guessed which spelling applied and guessed wrong — ~90 s of
timeouts that passed silently (issue 0481). The fix so far is a harness
(`DemoRole` + `expect_ready`) and three converged listeners; W7 is the campaign
to finish that across every example and to ENFORCE it with the same checker the
tests use, so an example cannot drift back.

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

## Remaining waves, REORDERED (2026-08-08)

The first six waves were planned before any of them ran. Seven waves of evidence
later, the ordering they implied is wrong in two specific ways, so the remainder
is re-sequenced here rather than worked in the order it was written.

**What the evidence changed:**

1. **A harness gap outranks the work it blocks.** W8 could not convert
   `xrce_ros2_interop`'s sleep — not for lack of an observable (the ROS 2
   listener prints `data:`) but because `Ros2Process` has no wait-until-pattern.
   The same gap will block every future ROS 2 sleep. Fixing the harness first
   turns a class of "cannot convert" into "convert".
2. **Gate first, converge second.** W7 proved it: the gate landed with 12
   baselined, then each convergence was VERIFIED as it happened, ending at 0.
   W5 shows the alternative — a convergence proposed from row counts, retracted
   on measurement. Any remaining wave that changes many files starts with the
   check that will judge it.

**The re-sequenced remainder:**

| order | wave | why here |
| --- | --- | --- |
| 1 | ~~**W9 (new)**~~ **LANDED** — `wait_for_output_count` on `Ros2Process` / `ZephyrProcess` | Unblocked W8's remainder as intended. |
| 2 | ~~**W8b**~~ **LANDED** — the 123 s of settle sleeps | Class 1 and 2 converted (24 s + 24 s); class 3 (~30 s) is genuine peer-discovery waiting with no observable, and stays, documented. Found issues 0483 and 0484 on the way. |
| 3 | ~~**W4** — tier-2 build scoping~~ | **DELIVERED by phase-340 W3**, not by this phase. Its reopen condition ("the run learns to select cells-for-this-lane PLUS everything uncoordinated") was built at fixture RESOLUTION rather than test selection. See the W4 section. |

**W8b is not a rewrite, it is W7 again with a different marker.** Those 123 s wait
on events that are never announced — `emulator.rs` sleeps 8 s for "bare-metal
boot + smoltcp init + zenoh connect" on an image that prints no boot marker. The
fix is the one W7 established: give the thing an observable line, put the marker
in the shared table, gate that examples print it, then wait on it. Attempting the
conversion without the observable is what makes a wait silently useless — the
exact defect issue 0481 catalogues.

**What is explicitly NOT next**, though it is the largest number in this
document: the ~2900 s of `check` that dominates a cold tier-1 run. That belongs
to phase-340 and phase-334 under their 2026-08-07 axis split, they have an
8-step work order, and this phase's opening reframe exists precisely to stay out
of it.

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

### W4 — UNBLOCKED, and DELIVERED ELSEWHERE (2026-08-10)

**W4 is done. Phase-340 W3 did it — on the SAME DAY this section declared the
item parked — by building the design this section named as the way out.**

The reopen condition recorded below was:

> either those files gain coordinates (they should not, mostly), or the run
> learns to select "cells for this lane PLUS everything uncoordinated", which is
> a different design than the one W4 assumed.

Phase-340 W3 (`5aa680c95`) built the second one. The insight is that the
narrowing does not belong at test SELECTION at all — it belongs at fixture
RESOLUTION:

```text
BUILD  skips row R  ⟺  row_coord(R) ∉ lane_coords   (fixtures-manifest.py --coords-from)
RUN    skips row R  ⟺  row_coord(R) ∉ lane_coords   (nros_tests::fixtures::lane)
```

Every test still RUNS. What narrows is which fixtures a test may find missing.
A test with no coordinate resolves nothing attributable to a manifest row, so it
never skips — `fixtures/lane.rs:66` states this as "a path that attributes to NO
row never skips (fail closed)". That is precisely "cells for this lane PLUS
everything uncoordinated", obtained without giving the 131 files coordinates they
should not have.

So the number below — 24 coordinate-bound files against 155 — was measured
correctly and reasoned about wrongly. It was read as "131 files cannot be
selected, therefore the run cannot be narrowed". The correct reading is that
selection was the wrong lever: nothing has to select those files, because
narrowing the fixture set already leaves them untouched.

The build half W4 actually asked for now exists too. `just build-test-fixtures
lane=tier2` is the tier-2 build, and `justfile:2241` records both the change and
its cost:

> That was false until phase-340 W3 and cost ~231 STALE failures when someone
> tried it: 0368 F8 had made `_require-fixtures` accept a `lane=tier2` stamp
> while `ci-matrix` still ran the WHOLE suite, so 34 of 47 coordinates were
> resolved and none of them had been built.

`ci-matrix` now exports `NROS_TEST_COORDS` from the SAME `nros_lane_coords_file
tier2` that scoped the build and the staleness gate — one computation reaching
three consumers, which is the invariant issue 0482 exists to protect.

**The lesson worth keeping is not about W4.** This blocker was re-confirmed with
a number on 2026-08-08, and `5aa680c95` dissolved it at 18:28 on 2026-08-08 —
the same day, from a phase that never mentions W4. Two phases owned different
halves of one lane and neither watched the other, so one wrote "still blocked"
into a roadmap while the other was landing the unblock.

That is a multi-session hazard, not a reasoning failure: nothing either session
could read would have told it. The cheap defence is the one this entry now
provides — when a blocker names a design as its way out, grep for that design
before re-confirming the block. `NROS_TEST_COORDS` was already in the justfile.
A parked item is a claim about the world and goes stale like any other.

The original analysis follows, kept because its measurement is still true and
its reasoning is a useful thing to have been wrong about.

### W4 — the blocker as recorded, and why the reading was wrong (2026-08-08)

Re-checked rather than inherited. The blocker is "every test cell-bound", and
the measurement is:

```
test files deriving cases from matrix::CELLS / interop::CELLS :  24
test files in packages/testing/nros-tests/tests             : 155
```

A coordinate-scoped RUN selects by coordinate. **131 files have no coordinate to
be selected on**, so scoping the run would not narrow it — it would silently drop
them. That is the same failure `lane-filter.sh` guards against on the platform
axis (issue 0357), one axis over.

This does not mean the 131 are wrong: phase-329's disposition pass established
that most are genuine one-offs — behaviour, boot, error and edge tests no cell
covers. A test without a coordinate is not a test missing metadata; it is a test
whose subject is not a matrix cell.

So W4 stays parked, and the condition to reopen is now precise: either those
files gain coordinates (they should not, mostly), or the run learns to select
"cells for this lane PLUS everything uncoordinated", which is a different design
than the one W4 assumed. Recorded so the next reader does not re-derive it.

### W4 — original verdict (2026-08-08)

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

### W7 — Example output conformance, enforced by the harness — COMPLETE (2026-08-08)

**Result: 12 divergent example listeners → 0, across five platforms and three
languages, with the gate that proves it.** `KNOWN_DIVERGENT` is empty; the gate
was verified to fire against an empty baseline, which is when a gate is most
likely to have gone decorative.

One correction to this item's original framing, worth keeping. It named
`zephyr.rs`'s `SERVER_READY_LAX = "Waiting"` as "the exact ambiguity issue 0481
is about". It is not. 0481 was a literal matching some binaries BY ACCIDENT;
this is a union that must match two legitimate spellings, because a zephyr
server cell's readiness depends on whether the node is rust (component-main
emits `"Waiting for messages"`) or C/C++ (the canonical server marker). One
constant cannot be both. The prefix is the honest encoding of "either of these",
and it is now documented as deliberate rather than left looking like a defect.

`NODE_READY_MARKER` was rebound to `output::WS_C_LISTENER_READY_MARKER` — a
byte-identical value, so nothing changes at runtime; what changes is that the
shared table owns the string instead of a file-local copy.

**Deliberately NOT done: per-cell markers keyed on each zephyr cell's language**,
which would remove the union entirely and is the right end state. It changes
which line each cell waits on, and no zephyr tree is provisioned on this host
(`third-party/zephyr` absent; provisioning is a west init plus SDK). A readiness
change that cannot be run is precisely the change that fails silently — the
lesson this entire work item exists to record.

#### Original statement of the item

Opened by the W1 follow-up. Fixing `rust_cyclone` turned out to be a one-word
disagreement between three implementations of the SAME standard ROS demo:

```
rust/listener   "Subscriber created for topic: /chatter"
c/listener      "Subscription created for topic: %s"     <- one word
cpp/listener    (none — only "Node created:" + a banner)
```

That is why a hand-picked literal matched two languages out of three and timed
out silently on the third, nine times over (~90 s, issue 0481).

**The divergence is wider than the three natives.** Across all 30 example
listener sources:

| spelling | files |
| --- | --- |
| `"Subscriber created for topic: …"` | 8 |
| `"Subscriber created"` | 6 |
| `"Subscription created for topic: …"` | 4 |
| `"Waiting for messages\n"` | 2 |
| `"Waiting for messages..."` | 1 |

**Five spellings for one role.** By contrast the TALKER role is already uniform —
21 of 21 print `"Publishing: …"` — which is the proof that convergence is
achievable and that the listener is the outlier, not the rule. Service servers
are nearly uniform (9 print `"Waiting for service requests"`, 5 also print
`"Service created: …"`).

**The campaign.** Every example is an implementation of the same standard ROS
demo, so its output should be a contract, not a per-file choice:

1. **Delivery lines already comply** and must not be touched — `"Publishing: …"`
   and `"I heard: […]"` mirror `demo_nodes_cpp`, and `TALKER_LOG_PREFIX` /
   `LISTENER_LOG_PREFIX` already pin them.
2. **Readiness lines are a nano-ros addition** and need an internal standard, one
   per role, which is exactly what `output::ready_marker(role, _)` now returns.
3. **Converge each role's examples onto that marker, additively.** Add the line;
   do not replace an existing banner. phase-277 slimmed banners and broke ~10
   tests — the constraint is real, and additive change respects it. Done for the
   three native listeners; 27 example sources remain, spanning freertos, nuttx,
   threadx, zephyr and esp32.
4. **Then enforce it with the SAME checker the tests use.** A gate maps
   `example dir -> DemoRole -> required marker` and asserts the source prints it.
   Tests call `expect_ready(role, …)`, the gate asserts the example satisfies
   that role — so the harness and the gate cannot disagree, because they read one
   table.

**Why this is the durable fix and the `output::` constants are not.** The
constants describe what the examples happen to print; nothing makes an example
keep printing it. `LISTENER_READY_MARKER`'s own doc records that this class
already bit once (issue 0471). A conformance gate inverts the direction: the
marker becomes the requirement, and an example that stops printing it fails at
`check-fast` rather than at a 30-second timeout nobody reads.

*Acceptance:* one spelling per role across every example; the gate green; and
`ready_marker`'s `lang` parameter provably unused by every arm — the collapse of
that branch is the measurable signal the divergence is gone.

*Care:* embedded listeners have their own test greps. Each converges with its
own check, like the natives did — no blanket rename.

### W8b RESULT — the settle survey, completed (2026-08-08)

The 123 s of "settle" turned out to be three different things, and the plan's
assumption ("events that are never announced") was right for only one of them.

**1. Converted and verified — `emulator.rs`, 24 s.** The assumption was FALSE
here: the RTIC images already printed `Waiting for messages on /chatter...` and
`Waiting for action goals...`, and `QemuProcess` already had
`wait_for_output_pattern`. Observable and API both existed; only the call site
still slept. Now waits per role — and the conversion immediately caught a wrong
marker (a service marker in front of an action server) that the 8 s sleep could
never have reported. **16/16 emulator tests pass**, where all 16 had been
skipping (issue 0483).

**2. Converted, and it found a 2 s product bug — `threadx_riscv64_qemu.rs`, 24 s
at six sites.** The listener is `examples/qemu-riscv64-threadx/c/listener`, which
W7 converged onto `LISTENER_READY_MARKER` and which `example_output_conformance`
now GATES, so the marker was guaranteed present. Converted per site (a blanket
replacement had already over-matched here once, and in `emulator.rs` had put a
service marker in front of an action server — both caught only by compiling or
running; the rule is per-site edits).

What the conversion exposed is the point of the whole work item. With the fixed
`sleep(4s)` gone, the per-cell numbers separated: c 1.35 s, cpp 1.45 s, **rust
5.31 s**. Filed as issue 0484, then diagnosed to a `tx_thread_sleep(200)` in the
RUST entry wrapper only (`nros-board-threadx/src/entry.rs`, two sites) — at
`TX_TIMER_TICKS_PER_SECOND = 100` that is exactly 2.00 s, the whole gap. C and C++
never paid it: their `main` calls the nros-c API directly and never enters that
wrapper. The delay was inherited ("matching the legacy per-overlay wait"), not
measured, and the link it waited for is already UP at 0.07 s — before the app
thread even starts.

Both sites deleted. The suite went **7.37 s to 1.505 s**, and the three languages
now land within 100 ms of each other (rust 1.407 s, c 1.465 s, cpp 1.504 s), which
is the property that should have held from the start: shared platform crate,
shared board crate, and a C API that thinly wraps the same Rust API. **The
asymmetry was the bug — the four seconds were only how it announced itself.**

That is the second time in this phase that deleting a fixed delay surfaced a real
defect rather than merely saving its duration (the first: splitting the pubsub
fold exposed `rust_cyclone` at 34 s, issue 0481). A sleep long enough to cover the
worst case is long enough to hide every case.

**3. Genuine settles — `interop_e2e`, the bridge tests, `params.rs`, ~30 s.**
These wait on PEER DISCOVERY, and no process announces it:

```rust
// Listener first — its subscription must be discoverable before the bridge's
// cyclone egress publisher matches over SPDP.
let mut listener = spawn_cyclone_listener(&listener_bin, domain);
std::thread::sleep(Duration::from_secs(3));
```

The event is "the remote matched me", which neither side prints.
`params.rs:147` is the clearest case: it ALREADY waits for a `Publishing`
marker, then sleeps 1 s more for "parameter service discovery propagation
through zenohd". The observable it wants does not exist on either endpoint.

Removing these needs a new observable (a bridge that logs when its egress
matches) or a discovery API to poll — different work from replacing a sleep with
a wait, and the honest boundary of this work item.

**Rule for new code, which is what W8 was really for:** never `sleep` toward a
condition a process announces. Where nothing announces it, say so in the comment
— every remaining sleep in class 3 now does.

### W8 — No unconditional sleeps: wait for a marker (NEW, 2026-08-08)

**The rule: a test must not sleep for a duration when it can wait for an event.**

Measured across `packages/testing/nros-tests/tests`: **189 s of unconditional
`sleep` at 56 call sites**, classified by what immediately follows:

| pattern | sites | seconds | verdict |
| --- | --- | --- | --- |
| settle / stabilisation | 43 | 123 s | may be genuine — needs a per-site observable |
| sleep-then-**kill** | 7 | 39 s | replaceable |
| sleep-then-**assert** | 6 | 27 s | replaceable |

The bottom two rows are the same defect W1–W7 chased, wearing the opposite
clothes. Those were TIMEOUTS — waiting for something that never came. These are
SLEEPS — not waiting for anything at all. Both substitute the clock for a
condition, and both are invisible because the test passes either way.

The replaceable shape is unmistakable once seen (`native_api.rs:806`):

```rust
let mut talker = spawn_native(...);
std::thread::sleep(Duration::from_secs(6));   // "long enough"
talker.kill();
// … count_pattern(&out, LISTENER_LOG_PREFIX) >= 2
```

The condition is ALREADY WRITTEN, three lines down, in the assertion. The sleep
is that same condition guessed at — and it costs its full duration even when
delivery landed in one second.

**Landed:** the five `native_api` sites (6/6/6/8/8 s = 34 s) now
`wait_for_output_count(LISTENER_LOG_PREFIX, 2, 20 s)` and kill on success. The
timeout carries the collected output in its error, so a failure reads exactly as
it did before.

**Remaining replaceable:** `nano2nano.rs:223` (3 s), `xrce_ros2_interop.rs:112`
(2 s) — different shapes, each needs its own read.

**The 123 s of settle is NOT bulk-convertible**, and pretending otherwise would
repeat W5's mistake. `emulator.rs:559` sleeps 8 s for "bare-metal boot + smoltcp
init + zenoh connect" on a QEMU image that prints no readiness marker. Converting
it requires the image to SAY it is ready — which is exactly what W7 did for
listeners, and the same argument applies: give the thing an observable event,
then wait on it. Survey first, per platform, and convert only where an event
exists.

*Acceptance:* zero `sleep-then-kill` / `sleep-then-assert` sites; the settle
sites each either converted or annotated with why no observable exists.

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
