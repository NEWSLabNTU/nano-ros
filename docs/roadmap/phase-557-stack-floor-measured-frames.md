# Phase 557 — Stack floors from measured frames, delivered per build

**Status (2026-11-13). OPEN: W1, W2, W2b, W3, W4, W5 and W6 are done; W3b
and W5b are open.** This phase tracks the implementation of
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
[issue 2496](../issues/2496-freertos-cmake-app-stack-no-system-toml.md),
[issue 2506](../issues/2506-stack-floor-walk-machine-outliner.md), and the
resolved [2461](../issues/archived/2461-stack-floor-freertos-stated-key.md),
[2466](../issues/archived/2466-stack-floor-ci-image-missing-c-frames.md)
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

- **Status:** DONE (2026-11-13, #3728)

**No pre-link edge is possible.** heap_4 hands out the task stack, and
`configTOTAL_HEAP_SIZE` is fixed at configure, before anything is linked
that could be walked. So the raise is a SECOND build. An image opts in with
`[image.<id>] stack_floor = "raise"` (`"check"`, the default, is the only
other word; a typo is refused). Its walk (`nros-stack-floor.py --raise`)
records a measured shortfall in the report instead of failing. `nros build`
reads that report and builds the image once more at the floor:

- **cmake road** — re-runs the configure and the build with
  `NROS_STACK_FLOOR_RAISE=NAME=VALUE` in the environment.
  `_nros_freertos_entry_app_stack` takes the larger of the stated value and
  the raise. The heap grows by the same amount
  (`NROS_FREERTOS_HEAP_RAISE_KB`).
- **cargo road** — rebuilds with the knob itself
  (`NROS_FREERTOS_APP_STACK_KB`) in the environment, then re-walks the
  image. The generated `[env]` rows are never `force`d, so the raise beats a
  stated value. The board's build script sizes the heap from the stack.

The raise is derived again by every build and stored nowhere: a cached
value is state that `system.toml` does not show. The build prints the value
it used and how to pin it:

- **cargo road** — prints the `[image.<id>.knobs.freertos] app_stack_kb`
  line to pin it.
- **cmake road** — has no `system.toml` spelling for this knob yet, so it
  says so and names the configure flag
  ([issue 2496](../issues/2496-freertos-cmake-app-stack-no-system-toml.md)).

If a raise still persists after the rebuild, that is a bug in the raise,
and the build fails.

The cmake road's check now keys by the `nros new` stamp
(`-DNROS_IMAGE_STACK_FLOOR_IMAGE`, passed on every configure by `nros
build`), which resolves
[issue 2461](../issues/archived/2461-stack-floor-freertos-stated-key.md).

Acceptance, measured (2026-11-13). Each image was given a temporary
allowance row:

- **C talker on mps2-an385-freertos (cmake), configured at 8 KiB.**
  - Opted in, it builds with 15 KiB (`.app_stack_bytes = 15360u`, heap +7 KiB).
  - Without the opt-in it fails with "Raise it: NROS_FREERTOS_APP_STACK_KB=15".
- **Rust talker (cargo), `app_stack_kb = 16`.**
  - Opted in, it builds with 26 KiB, and the re-walk reads "OPEN stack 26624 B holds the floor 26096 B".
  - Pinned with the printed line, it builds once with no raise.
  - Without the opt-in it fails with "Raise it: NROS_FREERTOS_APP_STACK_KB=26".

### W3 — PX4

- **Status:** DONE (2026-11-13, #3735). SITL is covered. The NuttX half was
  measured once and has no lane; that is W3b.

The placer runs the walk. `scripts/build/cargo-out-dir-headers.py --stack-floor
px4-uorb` arms the cargo command for the walk: target-scoped `-Csave-temps`,
`-fstack-usage` for cc-rs, and no `RUSTC_WRAPPER`. Neither flag changes the
code. After cargo, the placer walks the archive that cargo reported for
`nros-cpp`. It writes `NROS_STACK_FLOOR_OPEN/_SPIN` and their `_MEASURED`
into the placed `nros_cpp_config_generated.h`, inside the include guard. The
header keeps its mtime when the floors are unchanged.

The preset names both roles:

- **OPEN** = `nros_cpp_init`, `nros_cpp_executor_open_over_session`. The
  main task's `start` opens the session and borrows it for every other sched
  task.
- **SPIN** = `nros_cpp_spin_once`. One pass per work-queue `Run()`.

All three placers of the uORB archive spell `--stack-floor px4-uorb`: the
module's remedy text, `just px4 build-sitl-example`, and
`scripts/stage-px4-posix-archives.sh` (the release). The walker finds the
build's intermediates from cargo's messages (`--cargo-messages`, with
`--untargeted` when the command names no `--target`). It skips `--sysroot`
under `-Zbuild-std`.

At configure, `nros_px4_read_walked_floors` reads the header.
`nros_px4_check_thread_stacks` then checks STACK_MAIN against OPEN and each
work queue against SPIN:

- **Measured floor:** enforced like the executor's minimum. The minimum is
  still always enforced.
- **Unmeasured floor:** a WARNING, never a refusal.
- **Header with no walk:** a STATUS line saying only the minimum was checked.
- **The remedy** names the CONFIGURED value, inverting PX4's scaling. On
  64-bit SITL a 37,576 B floor is `CONFIG_WQ_RATE_CTRL_STACKSIZE=6500`, not
  37,576.
- **Configure dependency:** the header is now one, so a re-placed floor is
  checked again.
- **No issue id** appears in the user text any more.

Gate `check-px4-thread-stacks` has 33 checks. Eight are new:

- measured refusal on each thread;
- measured held;
- unmeasured warns;
- unmeasured never lowers the minimum;
- no-walk STATUS;
- the SITL remedy's configured value, both ways.

It also checks the wiring of all three placers. Mutations caught: the
measured branch disabled, and the unmeasured warning dropped. The gate holds
no floor of its own: its synthetic headers are shaped like the placer's
output.

Acceptance (SITL, measured 2026-11-13, `just px4 build-sitl-example`, PX4
1.17.0 at our pin):

| run | archive floors | result |
| --- | --- | --- |
| stock | OPEN 10,712 B (static 9,736), SPIN 4,576 B (static 4,160), both unmeasured | configure and build pass; `wq:rate_ctrl` is 30,876 B on SITL |
| test row: SPIN +30,000 B, measured | SPIN 37,576 B, measured | **refused at PX4's configure**, naming `wq:rate_ctrl` (30,876 B) and the measured floor |
| row removed | as stock | passes again; a further run keeps the header's mtime |

**NuttX fmu-v6x: measured once, no lane (evidence, not coverage).** The
recipe was phase-547's `31-nros-cross.sh`, re-run from that doc's text
against a stock `px4_fmu-v6x_default` configure of PX4 1.17.0 in this
worktree:

- `cargo +nightly-2026-04-11 build -p nros-cpp --target thumbv7em-nuttx-eabihf`
  with `-Zbuild-std=core,alloc,panic_abort`;
- features `alloc,log,panic-platform,rmw-cffi,platform-nuttx`;
- `NUTTX_DIR` = PX4's configured NuttX tree, Arm GNU 13.2.1;
- under `cargo-out-dir-headers.py --stack-floor px4-uorb`.

| profile | OPEN static (floor) | SPIN static (floor) |
| --- | --- | --- |
| release, `opt-level=3` | 7,528 B (8,288) | 3,672 B (4,040) |
| spike, `opt-level="z"`, fat LTO, 1 CGU | 77,240 B | 69,680 B: wrong, [issue 2506](../issues/2506-stack-floor-walk-machine-outliner.md) |

The O3 numbers agree with issue 2398's frames: SPIN 3,624 B. OPEN is lower
only because issue 2401 shrank `open_marshalling_properties` from 8,472 B to
240 B. Both floors are above PX4's stock sizes: OPEN's 8,288 B is above
the 2,048 B STACK_MAIN, and SPIN's 4,040 B is above `wq:rate_ctrl`'s 3,150 B.
The executor's 3,808 B minimum already refuses both stock sizes. A measured
row would name the walked floor instead. The `z` numbers come from the machine outliner's shared `OUTLINED_FUNCTION_*`
bodies conflating callers. They are a false HIGH, so an unmeasured image
only warns.

### W3b — A NuttX PX4 placer recipe and a lane that builds it

- **Status:** OPEN (blocked on
  [phase-542](phase-542-px4-supported-platform.md) W8.b)

W3 checks the NuttX threads wherever an archive is placed with
`--stack-floor px4-uorb`. But no in-repo recipe cross-builds the NuttX
archive: the one above is an out-of-tree spike script, re-run by hand. Per
"no road without a lane that builds it", the NuttX case is not covered until
a recipe and a lane exist.

Phase-542 W8.b already owns "prebuilt NuttX archives for one `px4_fmu-*`
target". It is deferred pending a maintainer ruling on hardware. So W3b is
not a second recipe: when W8.b lands, its placer passes `--stack-floor
px4-uorb`, and its lane asserts the floors are in the placed header. W3b
also needs [issue 2506](../issues/2506-stack-floor-walk-machine-outliner.md)
fixed, if that recipe ships at `opt-level="z"` as the spike did.

Acceptance: a lane builds the fmu-v6x archive through the placer, and the
placed header carries `NROS_STACK_FLOOR_OPEN/_SPIN`. A configure with a
test allowance row refuses stock STACK_MAIN by the measured floor.

### W4 — Zephyr CHECKs the main stack; key (c)

- **Status:** DONE (2026-11-12, #3586)

A `zephyr_final` post-build command checks `CONFIG_MAIN_STACK_SIZE`. `nros new`
stamps the template key and, when the template's floor is above the board
default, writes `stack.conf`. The fold refusal goes through the same
measured/unmeasured rule as a role's refusal.

Acceptance: the three nucleo projects reproduce their rows' static figures;
the action server fails until its `os_config` raises the stack, then holds.

### W5 — The cargo leaf CHECKs the board's stated stack

- **Status:** DONE (2026-11-13, #3686)

`nros build` of a single-package cargo leaf walks the image after cargo and
checks the board's stated stack, with the same measured/unmeasured rule.

Acceptance: a measured cargo-leaf image below its floor fails naming the
board's knob; an unmeasured one warns.

Scope (2026-11-13): the walk after cargo, plus FreeRTOS. On the cargo
road, only FreeRTOS boards state a stack today. The other platforms are W5b.

How it works (`packages/cli/nros-cli-core/src/cmd/stack_floor.rs`):

- **The stated stack.** The board's build script is the one place that
  resolves the stack knob's whole ladder, so it writes the number it
  resolved into its own `OUT_DIR`, as `nros-stack-stated.json`
  (`nros_board_common::stack_stated`). Every cross FreeRTOS board writes it
  through `freertos_build::run_overlay`: role OPEN, root `~app_task_entry_`,
  `app_stack_bytes`, knob `NROS_FREERTOS_APP_STACK_KB`.
- **Arming.** `nros build` gives the image's cargo command the walk's inputs:
  target-scoped `-C save-temps` and an LLD `-Map`, joined with the board's
  config rustflags; `-fstack-usage` for cc-rs; and an empty `RUSTC_WRAPPER`.
- **Finding the file.** After cargo links, `nros build` asks cargo again with
  `--message-format=json`, which rebuilds nothing. That names the build
  scripts that ran for this image, so a sibling feature set's `OUT_DIR` is
  never read.
- **Not checked.** A board that writes no file, or a platform outside the
  list, prints "not checked: the board states no stack". It is never a pass.
- **Off switch.** `NROS_STACK_FLOOR=OFF` skips the walk.

The walker gained three inputs for this road:

- **`~text` roots.** The FreeRTOS app task is a generic Rust function, one
  instance per board and runtime shape. A `~` root that matches nothing is
  refused.
- **LLD maps.** `rust-lld` writes no `LOAD` lines. The C members of each rlib
  are taken from the map and extracted with `llvm-ar`; Rust codegen units
  come from their own bitcode.
- **`--cfi-elf`.** The board rlib bundles newlib's `libc.a`, which has no
  `.su`. Its frames come from the linked image's `.debug_frame`. An FDE whose
  CFA leaves SP is not trusted.

Two hand-assembly rows were added to the table: `__aeabi_cdcmple`, and the
diverging `__assert_func` cut.

Measured on `examples/mps2-an385-freertos/rust/talker` (thumbv7m, zenoh):

| stack | allowance row | result |
| --- | --- | --- |
| 131,072 B (default) | none | holds the unmeasured floor 26,096 B (static 23,720 B) |
| 16,384 B | none | warns and builds: unmeasured, never enforced |
| 16,384 B | test row (not committed) | fails: "Raise it: NROS_FREERTOS_APP_STACK_KB=26" |
| 131,072 B | test row | holds the measured floor |

The bare-metal example prints "not checked: the bare-metal board
`mps2-an385-baremetal` states no stack on this road".

**Fix forward (2026-11-13, #3717): a caller-named `--target-dir`.** The
first L3 run after #3686 went red in `rust-rtos-link-check`, which builds the
talker with `nros build -- --profile nros-minsizerel --target-dir
target-link-check`. A caller-named target dir gets a cargo `build.build-dir`
per configuration (`<d>/nros-class/<key>/build`, RFC-0100 D13). The binary
is still uplifted to `<d>/<triple>/<profile>/`, but its intermediates are
not. The walk looked for `deps/` and `build/` beside the binary, found none
of the map's inputs there, and refused: "names no input under the link
directory".

The intermediates now come from cargo's own messages
(`--intermediates-from-build`): `<build dir>/<triple>/<profile>`, read off
the `out_dir` cargo reports for each target build script. That covers a build
dir set by `nros build`, by the caller or by a config file, so the walk is
never handed a dir derived from the target dir again.

Two more defects sat behind the first:

- **The json rerun rebuilt the image.** The `--message-format=json` rerun
  was cloned from the cargo command BEFORE the walk's flags were merged
  into it. So cargo rebuilt the whole image a second time without them,
  relinked over the ELF the walk reads, and doubled every walked build.
  The rerun is now the armed command plus `--message-format=json`.
- **Release profiles strip the frames.** A profile with no debuginfo
  (`nros-minsizerel`, `release`) makes cargo pass `-C strip=debuginfo`,
  which drops the `.debug_frame` that newlib's frames are read from, so
  `__assert_func` had no frame. The walk now sets the profile's own
  `CARGO_PROFILE_<P>_STRIP=none`, unless the caller set it. Those
  sections are not loaded, so the image's bytes on the target are
  unchanged.

Measured with the lane's command: the walk reads 104 CFI frames, and the
OPEN stack of 131,072 B holds the unmeasured floor of 25,824 B. The LTO
build's static walk is 23,476 B, against 23,720 B for debug. Alternating
`NROS_STACK_FLOOR` on, off, on, on, off, off in one target dir relinks
nothing after the first build of each kind, because cargo keeps both
variants of each unit. So the walk's flags do not thrash a shared dir.

### W5b — The other cargo-road platforms state a stack

- **Status:** OPEN

On the cargo road, ThreadX, NuttX and bare metal state no stack, so W5
reports their images as not checked. Each joins when its board build script
calls `stack_stated::emit` and its platform is added to `STATING_PLATFORMS`:

- **ThreadX (riscv64).** `nros_board_app_stack_size` is a hardcoded 512 KiB in
  the board's C. It needs a knob before anything can be checked against it.
- **NuttX.** The sched-task stack: `NROS_NUTTX_SCHED_TASK_STACK_BYTES`, or
  `[tasks.*.nuttx].stack_bytes`.
- **Bare metal (mps2-an385).** It states no stack at all, and the stack
  grows into RAM unguarded
  ([issue 2494](../issues/2494-mps2-baremetal-stack-unbounded.md)). The
  board must state one first.

The FreeRTOS sched tasks (`[tasks.*.freertos] stack_bytes`) are one more
role: W5 checks the boot app task only.

Acceptance: each platform's image is walked after cargo and checked against
its board's stated stack, with the same measured/unmeasured rule as W5.

### W6 — The twin lane: a stale allowance is detected

- **Status:** DONE (2026-11-13, #3663)

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
