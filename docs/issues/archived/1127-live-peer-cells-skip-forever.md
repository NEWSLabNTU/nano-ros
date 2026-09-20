---
id: 1127
title: "no live-peer cell has ever produced a verdict: the lane that carries them has not finished in 30 runs (issue 1136), and nothing would tell a permanent skip from a pass if it did"
status: resolved
type: bug
area: testing, rmw
severity: high
found: 2026-09-06
related: [0352, 0445, 0759, 0791, 0903, 1055, 1136]
---

# Reached by a lane, and the lane never gets there

`nros_tests::interop::CELLS` is the intent list for every test whose subject is
a LIVE ROS 2 peer (RFC-0051, phase-324). It has 18 rows, 17 of them `Runtime`
(one is a declared carve-out with no test), across 11 test binaries.

**They are wired into a lane.** The root `just test-all` (`justfile:2172`) runs
`cargo nextest --workspace` with **no exclude filter**, so all 11 are in it;
`just ci tier1` is preconditions + check + `rust-rtos-link-check` + `test-all`;
and `host-tests.yml` runs `just ci tier1` (line 266) on every push to `main`.

**That lane has ROS.** Both its jobs run in
`container: ghcr.io/newslabntu/nano-ros-ci:humble`, so the distro comes from the
image and a grep for `ros-humble` / `setup-ros` / `ROS_DISTRO` in the workflow
finds nothing. (This issue asserted the opposite in its second revision. It was
wrong; the container line is at `host-tests.yml:75` and `:138`.)

**And the lane never reaches the tests.** Measured over its last 30 runs on
2026-09-06: **0 success, 10 failure, 18 cancelled.** The last five failures die
at the same step, `Build workspace fixtures`, with

```
CMake Error at cmake/NanoRosEntry.cmake:426 (add_executable):
  Cannot find source file:

    LAUNCH_ARGS
```

— the CLI emits a keyword `nano_ros_entry` stopped parsing. That is **issue
1136**, and it is the mechanism: `just ci tier1` never starts, so no interop
cell has produced a result in CI, ever.

## Two failures, and only one of them is fixed by 1136

**The first is 1136's**: today the cells cannot run. Fix that and the lane
reaches them.

**The second is this issue's, and it outlives the fix.** When the lane does
reach them, each cell will report a pass, a failure, or a `skip!` — and
**nothing distinguishes a cell that has skipped on every host since the day it
was written from one that is covered.** A skip is not a verdict. This is issue
0445's absorbing-verdict class one lane over: there a STALE fixture replaces
whatever the runtime would have done with a message explaining itself, and 0444
hid behind it for exactly that long. Here a green tick does the same job.

`matrix_fixture_coverage.rs` G1 is the gate closest to it, and its doc comment
claims exactly this ("A Runtime cell nothing runs … fails here"). What it
asserts is `tests_dir.join(format!("{}.rs", c.test)).is_file()` — the FILE
exists. Five gates surround a cell (G1–G5) and **not one asks whether the cell
has ever produced a result.** They are all statements about declarations.

## The second problem: no focused runner for most of them

`--workspace` can reach a binary but cannot let a human run *one cell against a
live peer*, which is what verifying any of this requires. Of the 11 binaries,
three are named by a recipe:

| test binary | cells | focused runner |
| --- | --- | --- |
| `interop_e2e` | 5 | `just native test-ros2`, `just native test-ros2-lifecycle` |
| `xrce_ros2_interop` | 2 | `just xrce test-ros2` |
| `params` | 1 | `just native test-ros2-params` |
| `graph_interop` | 2 | **none** |
| `qos_zephyr_ros2_interop_e2e` | 1 | **none** |
| `qos_override_e2e` | 1 | **none** |
| `rust_multi_node_per_node_graph` | 1 | **none** |
| `cpp_multi_node_entry` | 1 | **none** |
| `declarative_bridge_zenoh_to_cyclonedds` | 1 | **none** |
| `declarative_bridge_zenoh_to_xrce` | 1 | **none** |
| `bridge_zenoh_to_cyclonedds` | 1 | **none** |

`just native test-all` aggregates the three, and is itself called by nothing.
`just native test` and `just test-integration` (different lanes) *do* exclude
the ros2 groups — correctly, on their own terms; misreading that exclusion as
the root sweep's produced the first version of this issue. Note also that
`host-tests.yml`'s own header comment still describes the integration job as
running `just test-integration`, which is the excluding recipe, while line 266
runs `just ci tier1`, which is not.

## Why nobody noticed

`matrix_fixture_coverage.rs` G1 is the gate closest to this, and its own doc
comment says "A Runtime cell nothing runs … fails here". What it asserts is:

```rust
if !tests_dir.join(format!("{}.rs", c.test)).is_file() {
```

The test FILE existing. G2 checks the build channel can build the coordinate,
G3 checks the build recipe's spelling, G4 checks the peer declaration, G5
checks a `fixtures.toml` row produces the coordinate — five gates around the
cell, and **not one of them looks at whether the cell has ever produced a
result.** They are all statements about declarations.

This is the repo's recurring shape stated in phase-393's own closing warning:
*a slot's EXISTENCE reads as coverage*. Here it is one level up, and the
existence in question is a green tick.

## What it costs, concretely

Phase-381 shipped twelve `rmw` graph slots: produced, reachable from three
languages, mutation-tested, `check-api-parity` clean. **The feature did not
work at all** — `z_liveliness_get` is an interest, so a sweep saw an arbitrary
handful of the domain's tokens. Issue 0903 was several stacked defects on top
of a mechanism that could not work, and none of it manifests except against a
real peer. `graph_interop.rs` is the committed form of that lesson and it has
never executed.

Cyclone's graph reader (phase-381 W5) has never run against a live participant
either; the cell says so in a comment.

## The second half — ROS-2-facing tests with no cell at all

Eleven test files call `ros2_env_setup` or drive the `ros2` CLI and do not call
`interop::assert_test_bound`:

```
cpp_c_param_live_read_e2e  entry_e2e  param_live_read_e2e  ros2_action_e2e
ros_editions_bridge  ros_editions_smoke  ros_editions_nano_interop
ros_editions_e2e  output_marker_gate  workspace_features_e2e
zephyr_leaf_staleness
```

`ros_editions_*` is the docker edition axis and is deliberately not a cell
(CLAUDE.md says so). The others are not accounted for anywhere.
`ros2_action_e2e` is the notable one: **actions have no interop cell**, and
issue 0902 reports action goals completing between 20 % and 90 % of the time on
the same build.

## Why the box makes this structural, not lazy

This host has no ROS: `/opt/ros` does not exist, `ros2` is not on PATH,
`ROS_DISTRO` is empty. ROS 2 Humble lives in the `ros2` distrobox, and the
standing rule (issue 0759) is that a box in play means EVERY job runs in the
box on its OWN tree, because the compiler and libc differ and the artifacts are
shared with nothing checking they agree.

So a live-peer lane is not "add `--test graph_interop` to a recipe". It is a
lane that runs inside the box against the mirror tree, and that mirror is
`/mnt/wd/data/projects/nano-ros-box`, currently **323 commits behind** `main`
(at `d1d88f660`, 2026-09-04). There is also a `nano-ros-box-box` at
`ea9fbfae9` (2026-08-30) — a mirror of the mirror, from a sync run that
started inside the box tree. That is debris and should be removed once someone
confirms it holds nothing unique.

## Direction

Not "wire the tests in" — they are already wired in, and that is the point.
Nor a red lane, which has no signal capacity (the failure mode CLAUDE.md
names). The order that works:

0. **Fix issue 1136** — until `build-workspace-fixtures` configures, nothing
   below can be observed in CI at all.
1. A box-resident recipe that runs ONE cell end to end and reports honestly.
2. Run each cell once, by hand, and record the verdict per cell — a cell that
   fails is a finding, not a blocker.
3. Only then a lane, and only over the cells that passed, so the lane starts
   green and a red means something.
4. A gate that a Runtime cell's test binary is named by at least one FOCUSED
   recipe — what G1's doc comment claims to be, and what `--workspace` cannot
   substitute for.
5. **The durable fix is making a permanent skip visible.** A cell that has
   never produced a non-skip result on any host is indistinguishable from a
   passing one today. Whatever form that takes — a per-cell last-verdict
   record, a skip budget, a report — it is the only one of these five that
   stops the problem recurring.

Phase 433 owns this work.

## Item 5 LANDED 2026-09-06 — a verdict is recorded, or it never happened

`.config/interop-verdicts.toml` + `scripts/check-interop-verdicts.py`
(`just check interop-verdict-ledger`, fast lane; `just interop-verdicts` for
the report). Design, and the four shapes rejected, in
[phase-433 W5.a](../../roadmap/phase-433-rmw-live-verification.md).

The shape: **absence of an entry means NEVER RUN.** A cell that has met a peer
has a dated entry naming the cases, the verdict, the command and what was
observed; every other cell is unproven by default, so nothing here can be
satisfied by a cell that never runs and a newly added cell needs no line.
Seeded with W1's two real verdicts — `native-graph-rust-zenoh-r2n` PASS,
`native-graph-rust-cyclone-r2n` FAIL (issue 1137). **2 of 17.**

Three properties this had to have, and how:

* **Not satisfiable by absence.** The default is "never", not "fine". A
  `fail` verdict must name an OPEN issue, so a finding cannot be parked.
* **Not red for a legitimate skip.** The gate checks the ledger's INTEGRITY,
  never a host's capability, so it is green on a laptop with no ROS. Age is
  reported in days, not expired: an expiry over cells only a ROS-carrying lane
  can refresh would go red for everyone and stay red.
* **Not a merge-queue serialiser (0883/0884).** The file holds only POSITIVE
  claims, written at most once per cell — 17 edits ever, not one per pull
  request. There is no shared generated line for a PR to touch.

The load-bearing rule, and the reason a junit-derived skip streak cannot work:
**a binding test is not evidence.** Ten of the eleven live-peer binaries carry
`cases_bound_to_interop_cells` — body is `interop::assert_test_bound(...)` and
nothing else — which passes on any host, with no peer, forever. So "did this
binary produce a non-skip result" is TRUE for all of them on a machine with no
ROS at all, and a mechanism reading that as coverage would be this very bug
wearing the fix's clothes. The gate refuses an entry citing one, and
`--record`/`--after-run` exclude them when reading a junit. (Measured across
the eleven: exactly one such test in ten of them, none in `graph_interop`,
whose binding call sits inside the real cases.)

Where a human sees it: the gate's own OK line on every push
(`2/17 … 15 NEVER RUN`), `just interop-verdicts`, and one line at the tail of
every sweep via `_test-summary` — which distinguishes the two runs that used
to print the same thing:

```
Real failures: 0 / 0 total failures
Live-peer interop: 2/17 cells have ever produced a verdict. This run reached
1 of their binaries — 0 produced a result, 1 skipped wholesale.
```

versus, when the case actually ran, the same header followed by `1 produced a
result, 0 skipped wholesale` and the cell named for recording.

**Still open in this issue:** item 2 (run the other 15 and record them) and
item 3 (a lane over the ones that pass) — phase-433 W2/W5. The ledger is where
W2's verdicts land, instead of a prose table nothing can check. This mechanism
cannot tell that a hand-written entry is TRUE, only that it is well-formed and
cites real non-binding cases; and it cannot know when a recorded verdict goes
stale, which is what the scheduled lane is for.

## Resolution (2026-09-20) — every clause of the title is now false, MEASURED

The title says three things. Each was true when it was written and none of them
is true today, which is its own defect on an open issue: it has been aiming
readers at a claim that stopped holding.

**"No live-peer cell has ever produced a verdict."** `python3
scripts/check-interop-verdicts.py --report`:

```
  25 of 28 Runtime cells have ever produced a verdict; 3 have not.
```

Twenty-five recorded PASSes, dated 2026-09-07 through 2026-09-18, each naming
the cases it ran, the command, where it ran and what was observed.

**"The lane that carries them has not finished in 30 runs (issue 1136)."**
`LAUNCH_ARGS` came back — `cmake/NanoRosEntry.cmake:116-141` carries both the
reason and the `cmake_parse_arguments` keyword list (phase-433 W0), so the
fixture configure that killed every run no longer fails. (Issue 1136 itself
stays open for its remaining half: nothing yet compares the keyword set the CLI
EMITS against the set `nano_ros_entry` PARSES. That is a producer/consumer gate,
not this issue.)

**"Nothing would tell a permanent skip from a pass if it did."** That was item 5
and it is the one that had to exist; it landed 2026-09-06 and is
`.config/interop-verdicts.toml` + `scripts/check-interop-verdicts.py` (`just
check interop-verdict-ledger`, fast line). The three cells below are not hidden
by it — they are NAMED by it, on every run, as `NEVER`, which is precisely the
property this issue asked for.

### The Direction list, item by item

| # | what it asked for | where it landed |
| --- | --- | --- |
| 0 | fix issue 1136 so the lane reaches the cells | `cmake/NanoRosEntry.cmake:116-141` (phase-433 W0) — the keyword is restored and wired |
| 1 | a recipe that runs ONE cell end to end | the focused recipes: `just freertos test-ros2` (`just/freertos.just:404`), `just zephyr test-ros2-cortex-m` (`just/zephyr-dev.just:443`), `just native test-ros2-multinode-rust` (`just/native.just:1036`), and the rest |
| 2 | run each cell once and record the verdict | 25 of 28 recorded, all `pass`; see the report above |
| 3 | a lane over the cells that passed | `.github/workflows/live-peer.yml` — membership DERIVED from the ledger (`--list-passing`), schedule + dispatch only, split host/board by each cell's own platform coordinate (phase-441 W4) |
| 4 | a gate that a Runtime cell is named by a FOCUSED recipe | `scripts/check-interop-cell-runners.py` (phase-433 W4) — what G1's doc comment claimed to be |
| 5 | make a permanent skip VISIBLE | the ledger, `just interop-verdicts`, the `_test-summary` tail line, and `check interop-verdict-ledger` on the fast line |

### The three cells with no verdict, and why each is OUTSIDE this issue

None of the three is blocked by anything this issue describes. Each needs one
run on a host that has the peer and the board, and each is already owned by the
roadmap item that shipped it:

* **`native-multinode-rust-cyclone`** (`rust_multi_node_per_node_graph`,
  `interop.rs:509`). Host runner, no board, no SDK: it needs ROS 2 with
  `rmw_cyclonedds_cpp` and one `just native test-ros2-multinode-rust`. Nothing
  blocks it. Its driving issue, 1269, is RESOLVED and says so itself
  (`docs/issues/archived/1269-…:37`: "It needs a live ROS 2 peer and has NOT
  been run here — it starts life with no verdict, as every new live-peer cell
  does"). The sibling zenoh cell passing says nothing about it, deliberately:
  Cyclone announces nodes through `ros_discovery_info`, not liveliness tokens.

* **`zephyr-cortex-m-pubsub-c-zenoh`** (phase-441 W1;
  `docs/roadmap/phase-441-rmw-on-target-verification.md:199-213` already records
  "**A live verdict is still owed**"). Needs a Zephyr SDK, `qemu-system-arm` and
  a ROS host. One MECHANICAL blocker beyond that, worth naming because it is not
  obvious from the cell: recording it also needs a `BOARD_FIXTURE_NARROWING` row
  (`scripts/check-interop-verdicts.py:232`), which today holds only
  `zephyr-qos-rust-zenoh`; a recorded board cell missing from it is a hard
  `LedgerError` by design.

* **`freertos-mps2-pubsub-c-zenoh-n2r`** (phase-441 W3;
  `…phase-441-…:413` — "the cell is LANDED and has NEVER MET A PEER … which is
  what 'never run' is supposed to look like here, not an oversight", and
  `:490-494` states the one command left). Needs ROS 2, `arm-none-eabi-gcc`,
  `qemu-system-arm` and the FreeRTOS + lwIP trees — and its fixture row
  `workspace-c-freertos` needs ament for `std_msgs`/`example_interfaces`, so
  BOTH halves want the same ROS host. It additionally cannot enter the
  `live-peer.yml` board job as it stands: that job's image provisions only
  `zephyr|qemu` and refuses a `freertos` scope loudly on purpose
  (`live-peer.yml:430-444`) — "give that board its own job (or its own image)".

The `board` job is NOT dead by construction, which a reading of the ledger's
three `NEVER` rows might suggest: `zephyr-qos-rust-zenoh` is platform
`ZephyrNativeSim`, classified `board`, and has a recorded pass — so
`has_board == 'true'` today and that job runs. (Its current red is issue 1364,
the Zephyr CI image having no TOML parser; a lane problem, not a cell problem.)

### What this issue can no longer do for anyone

Its subject was the MECHANISM — a cell that has never run must not read as
covered. That mechanism exists, is on the fast line, and reports the three
outstanding cells by name on every run. Keeping the issue open would make it a
duplicate of the ledger's own output, and of two roadmap acceptance items that
already state what is owed. Closed here; the residue is tracked by the fix.
