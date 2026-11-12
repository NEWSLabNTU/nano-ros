# Phase 557 — Stack floors from measured frames, delivered per build

**Status (2026-11-13). OPEN: W1, W2 and W4 have landed, and W6 lands with
this doc; W2b, W3 and W5 are open.** This phase tracks the implementation of
[issue 2400](../issues/2400-stack-floor-from-measured-frames.md): one producer
per target computes the floor of a thread's stack from the frames that thread
runs, and every gate reads that producer instead of holding its own number.
The design and every ruling are recorded in the issue; this doc tracks the
work items and their acceptance. Each item is claimed with
`just claim issue-2400-wN` before it starts.

Rulings in force (issue 2400 "Rulings"):

- Every build measures the floor:
  `floor = ceil8((static + allowance) x (100 + margin) / 100)`.
- The build CHECKs by default. A MEASURED image (one whose key has an
  `[[allowance]]` row) fails when its floor exceeds its stated stack, or when
  its walk cannot run. An UNMEASURED image warns and is never refused.
- Nothing lowers a stack. Raising one automatically is opt-in (W2b).
- Allowances are per image key and may be negative. The key is (c): `nros new`
  stamps `[image.<id>] stack_floor_image = "<template>/<lang>/<rmw>@<board>"`.
- The 32 KiB `NROS_RUST_MAIN_STACK_FLOOR` stays until measured rows cover the
  boards.
- No message a user can see names an issue, phase or RFC id.

Related: [issue 2402](../issues/2402-rmw-vtable-slot-binding-stack-walk.md)
(the allowance exists because a static walk cannot bind a vtable call to its
slot), [issue 2414](../issues/2414-stack-floor-bitcode-store-under-sccache.md)
(the deferred bitcode store),
[issue 2460](../issues/2460-stack-floor-mps2-zephyr-walk-refused.md),
[issue 2461](../issues/2461-stack-floor-freertos-stated-key.md), and the
resolved [2466](../issues/archived/2466-stack-floor-ci-image-missing-c-frames.md)
and [2473](../issues/archived/2473-stack-floor-table-needs-toml-parser.md).

---

## Work items

### W1 — The walker, the allowance table and strict mode

- **Status:** DONE (2026-11-10, #3479; rows re-measured after issue 2401 in #3499)

`scripts/nros-stack-floor.py` walks a role's deepest path from stable
`-C save-temps` bitcode through the pinned `llc`, GCC `.su` and `.ci` files,
and link-map inputs. `config/stack-floors.toml` holds the allowances, the
walk policy and the margin, and no floors. `--validate-table` (fast line,
`check stack-floor-table`) checks the table offline.

Acceptance: the walk reproduces issue 2398's direct-call measurement on
nros-cpp (15,728 B against 15,788 B), and each nucleo row's static figure.

### W2 — The cmake road CHECKs the FreeRTOS carrier

- **Status:** DONE (2026-11-11, #3556)

`cmake/NanoRosStackFloor.cmake` measures every Corrosion import and C/C++
target in the link closure, and checks `app_stack_bytes` post-build.

Acceptance: a 4,096 B stack with a test allowance row fails with the knob in
its own unit; without the row it only warns.

### W2b — Raise the stack automatically, opt-in

- **Status:** OPEN

A pre-link edge that gives the carrier the floor before it compiles, so an
image can opt into "raise to the floor" instead of CHECK. It never lowers.

Acceptance: an opted-in measured image whose stated stack is below its floor
builds with the floor, and an image that did not opt in still fails.

### W3 — PX4

- **Status:** OPEN

The placer runs the walk, writes `NROS_STACK_FLOOR_OPEN/_SPIN` into the placed
header, and `nros_px4_read_stack_floor` checks STACK_MAIN and the work-queue
stacks at configure. `check-px4-thread-stacks` reads that value and holds no
literal.

Acceptance: a PX4 image whose STACK_MAIN is below its measured floor is
refused at configure, and the gate holds no literal.

### W4 — Zephyr CHECKs the main stack; key (c)

- **Status:** DONE (2026-11-12, #3586)

A `zephyr_final` post-build command checks `CONFIG_MAIN_STACK_SIZE`. `nros new`
stamps the template key and, when the template's floor is above the board
default, writes `stack.conf`. The fold refusal goes through the same
measured/unmeasured rule as a role's refusal.

Acceptance: the three nucleo projects reproduce their rows' static figures;
the action server fails until its `os_config` raises the stack, then holds.

### W5 — The cargo leaf CHECKs the board's stated stack

- **Status:** OPEN

`nros build` of a single-package cargo leaf walks the image after cargo and
checks the board's stated stack, with the same measured/unmeasured rule.

Acceptance: a measured cargo-leaf image below its floor fails naming the
board's knob; an unmeasured one warns.

### W6 — The twin lane: a stale allowance is detected

- **Status:** OPEN

An allowance is the twin's peak minus the static walk at the time it was
measured. When a frame on the walk shrinks and the row stays, the floor falls
below the peak and every build still says "holds". Only a run can see it.

`scripts/stack-floor-twin.py` (`just zephyr stack-floor-twin`) re-measures
every row whose key is a template key on a board with a Renode twin. It runs
`nros new` for the template, then builds the image with the thread analyzer
and a 64 KiB main stack. It runs the image in the twin and reads the main
thread's peak. The build's own static walk comes from
`nros/stack-floor/report.json`, which the walker now writes (`--report`).
The lane fails when the peak is above the floor, and when
`bytes != peak - static`. Its message names the direction and prints the
re-measured row.

The build walks only when the image relinks, so the lane re-runs the build's
generated `check.cmake` before it reads the report. A table edit alone then
reaches the verdict. Measured: a JSON row set to −4,300 B failed as
"under-states by 64 B".

Acceptance: the lane runs every nucleo row and its verdicts are recorded
below. A row it finds stale is re-measured in the same change. The
self-test runs on the fast line, inside `check stack-floor-table`.

First run (2026-11-13). Renode is deterministic: the C talker's main thread
read the same peak on all 14 rounds, and at both its own 16 KiB stack and the
lane's 64 KiB.

| row | twin peak | static | row bytes | peak − static | verdict |
| --- | ---: | ---: | ---: | ---: | --- |
| `action-server/rust/zenoh@nucleo-f767zi-zephyr` | 30,256 | 21,780 | +8,476 | +8,476 | exact |
| `talker/rust/zenoh@nucleo-f767zi-zephyr` | 28,888 | 22,212 | +6,676 | +6,676 | exact |
| `talker/c/zenoh@nucleo-f767zi-zephyr` | 11,876 | 16,112 | −4,416 | −4,236 | **stale, unsafe by 180 B** |

The C talker's peak rose 180 B since 2026-11-10 while its walk did not move,
so the growth is on an indirect path the walk charges rather than follows
(issue 2402). Its old floor, 12,872 B, still held the peak, but only through
the margin. The row is re-measured to −4,236 B (floor 13,064 B, still under
the image's 16 KiB default, so `nros new` writes no `stack.conf`), and the
lane then reads it exact.
