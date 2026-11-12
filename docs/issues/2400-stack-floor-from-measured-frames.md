---
id: 2400
title: "Three stack floors are each stated a different way: one is computed from a value size, one is a hand-set 32 KiB, one is a measured FreeRTOS peak. None is derived from the frames the thread actually runs"
status: open
type: tech-debt
severity: medium
area: [memory, zephyr, px4, nuttx, freertos, build]
found: 2026-11-10
related: [issue-2360, issue-2365, issue-2398, issue-2359, issue-0961, issue-1146, phase-548, phase-557]
---

**Home: [phase-548](../roadmap/phase-548-three-languages-every-board.md)**, beside
issue 2360, whose measurements are this issue's main input. **The work items
and their status: [phase-557](../roadmap/phase-557-stack-floor-measured-frames.md).**

## What

A thread that runs nano-ros registration or an executor pass needs a minimum
stack. Three places state that minimum today, and each states it differently:

| floor | where | how it is stated | value |
| --- | --- | --- | --- |
| `NROS_EXECUTOR_MAIN_STACK_MIN` | `nros-build-helpers/src/cpp.rs` and the `nros-c` sizes-header templates (issue 0961) | **computed**: 2 x `NROS_EXECUTOR_VALUE_SIZE`. That covers two copies of one value on the stack and nothing else | 3,808 B on fmu-v6x (`thumbv7em-nuttx-eabihf`) |
| `NROS_RUST_MAIN_STACK_FLOOR` | `zephyr/CMakeLists.txt` (issue 2359), checked by `check-zephyr-rust-main-stack` | **hand-set** from one Renode peak (a talker, 28,280 B) | 32,768 B |
| `app_stack_bytes` | `nros-board-common` `freertos_config` (issue 1146) | **measured** high-water over 8 FreeRTOS images, rounded up | 131,072 B |

The PX4 refusal in #3414 (issue 2398) reads the first floor. Issue 2398
measured that floor to be about a quarter of what the open path needs on that
target: the open path alone is ~15.3 KB before any backend frame.

So a thread a gate accepts can still overflow. The number a gate checks says
nothing about the frames that thread runs.

## Measured inputs that exist

- **Runtime peaks** (issue 2360, nucleo-f767zi Renode twin, `CONFIG_INIT_STACKS`
  plus the thread analyzer, Rust + zenoh, main thread, after #3381): talker
  27,248 B, action server 29,184 B, action client 30,600 B. These are 1.4 to
  4.7 KiB under the 32 KiB floor. A graph-querying image
  (`ZenohSession::names_and_types_filtered`, a 19,620 B frame) is not measured.
- **Static frames** (issue 2398, `-Z emit-stack-sizes` +
  `llvm-readobj --stack-sizes`, thumbv7em, O3/Oz): `Executor::open_in`
  2,840/2,872 B, `init_rmw_over` 2,128 B, `CffiRmw::open_with_rmw` 648/600 B,
  `open_marshalling_properties` 8,472/8,464 B, `nros_cpp_init` 56/24 B. One
  spin pass is 2.8 to 4.2 KB.
- **FreeRTOS** (issue 1146): `uxTaskGetStackHighWaterMark` at the end of the
  register pass, worst of 8 images 36,152 B. Every image prints its own peak at
  boot.

## Proposal

Keep **one producer** per target. Every gate (the Zephyr Rust main-stack check,
the PX4 thread-stack refusal, the C/C++ `#error` on `CONFIG_MAIN_STACK_SIZE`)
reads that producer's output and does not hold its own number.

1. **Static bound.** At build time, take the archive's `.stack_sizes` (already
   emitted for issue 2398's measurement) and sum the worst path over the
   registration and open call chains. That gives a per-target, per-feature
   value that moves when a frame moves. Recursion and indirect calls through
   the RMW vtable stop the sum. Each such call is stated as a named allowance
   with the measurement it came from, never left out silently.
2. **Runtime check.** The Renode nucleo twin (and the FreeRTOS boot print)
   measures a peak for each covered image. A lane asserts `static bound >=
   measured peak`. If the bound is under the peak, the static walk missed an
   edge. That is a defect in the walk, not a reason to raise a constant.
3. **Delivery.** The value goes in the per-build sizes header beside
   `NROS_EXECUTOR_VALUE_SIZE`, the same header #3414's
   `nros_px4_read_stack_floor` already reads (issue 1824). PX4 then needs only
   its producer changed, not its reader. Zephyr reads the same header instead
   of `set(NROS_RUST_MAIN_STACK_FLOOR 32768)`. `2 x NROS_EXECUTOR_VALUE_SIZE`
   stays as a necessary-condition `#error`. It stops being called the floor.

Open choices, left to whoever takes it:

- Whether the static walk runs in `nros-build-helpers` (it has the archive) or
  as a `nros` CLI step after link (it has the final image, so inlining is
  settled).
- Whether the floor is one per image, or one per thread role (registration /
  executor pass / work queue). PX4 has two roles on two threads (issue 2398);
  Zephyr Rust has one.

## Acceptance

- One producer of the floor value. `check-zephyr-rust-main-stack` and
  `check-px4-thread-stacks` read it, and neither holds a literal.
- On the three nucleo Rust images in the table above, the derived bound is at
  or above the measured peak, and within a stated margin of it.
- A frame grown on purpose (a test mutation) moves the derived floor, and a
  board conf below it is refused.

## Measured: a static floor alone cannot be both sound and tight (2026-11-10)

`scripts/nros-stack-floor.py` walks a thread role's deepest path from the
compiler's frames. The inputs are `.stack_sizes` (rustc `-Z emit-stack-sizes`),
GCC `.su` (Zephyr `CONFIG_STACK_USAGE`), call edges taken from relocations,
and indirect-call sites taken from the disassembly.

**It agrees with issue 2398 on direct calls.** On `nros-cpp` for thumbv7em at
O3, counting direct calls only: OPEN (`nros_cpp_init`) 15,728 B and SPIN
(`nros_cpp_spin_once`) 3,592 B. Issue 2398 measured 15,788 B and 3,624 B.

**It cannot follow indirect calls.** On the nucleo-f767zi Rust zenoh action
server, the walk runs from `rust_main` over all 25 archives plus 310 `.su`
files. Issue 2360's Renode twin measured that same code at 29,184 B:

| walk | OPEN | vs 29,184 B |
| --- | ---: | --- |
| direct calls only | 18,112 B | under |
| fold1, indirect targets scoped to zenoh/zpico | 26,232 B | under by 2,952 B |
| fold1, every address-taken target | 45,776 B | 57 % over |
| fold to a fixed point | does not converge | zenoh's callbacks reach their own dispatch |

Every indirect site is charged the same bytes: the deepest address-taken
target in scope. With no scope, that target is the graph query
`ZenohSession::names_and_types_filtered` (19,656 B frame), which this image
never calls. It is charged at the log sink's call site as well as at
session open. No static input binds a call site to the slot it loads. That
is issue 2402.

**Ruling.** A floor is `max(scoped static walk, measured twin peak)` plus a
stated margin. A role with no twin (PX4's executor-pass thread) gets the
static walk plus a stated per-backend indirect allowance, and that allowance
names the issue that would retire it (2402). The static half catches a frame
that grows on a direct path at build time. The twin covers the indirect
half.

**What real images needed** (all now in the walker):

- `-Zbuild-std`. The prebuilt rustup `core`/`alloc`/`compiler_builtins`
  carry no `.stack_sizes`, which left about 40 reached functions with no frame.
- Stated cuts for the panic path and for `core::fmt`, which really is
  recursive through `dyn Write`.
- GCC naming. A symbol `foo.isra.0` has its `.su` entry under `foo.isra`, and
  identical-code folding gives one body several names with a single `.su`
  line.
- `--frame` for the C that cargo's `cc` compiles under Zephyr. Zephyr's cmake
  sets `CFLAGS_<triple>`, so `-fstack-usage` cannot be added there.
- Fold now refuses a target whose own walk is refused. It used to skip that
  target, and that is how `zpico_open` dropped out silently.

The per-target table is `config/stack-floors.toml`. Its first entry is the
image above, OPEN: static 26,232 B, measured 29,184 B, margin 10 %, floor
32,104 B, which fits inside the board's 32 KiB. `--validate-table` checks it
offline, and `--table --entry` reproduces 26,232 B on the built image. How
the measure lane runs, and how the floor reaches the sizes header, is still
open.

## Rulings (2026-11-10)

Decided by the maintainer:

- **Measured in every build, stable.** Frames come from `-C save-temps`
  bitcode, run through the pinned toolchain's own `llc`, with no unstable
  flag and no build-std. This is not a committed table of floors and not
  `RUSTC_BOOTSTRAP`. Measured: OPEN/SPIN identical to `-Z emit-stack-sizes` on
  nros-cpp (15,828 / 3,692 B), on the nucleo image (26,232 B), and under LTO
  (22,564 vs 22,560 B).
- **The floor is a committed addend, not a `max`.** Per build:
  `floor = ceil8((static_now + allowance) × (100 + margin) / 100)`. The
  allowance, per (RMW backend, target, role), is the twin peak minus the
  static walk at the time it was measured, and it names issue 2402. With
  `max(static, peak)`, a fixed old peak hides direct-frame growth.
  `config/stack-floors.toml` holds the allowances, the walk policy and the
  margin, and no floors.
- **A missing allowance** gives the static floor plus a loud warning naming
  issue 2402. An opt-in strict mode refuses instead, and CI runs strict
  wherever an allowance exists.
- **Zephyr and cargo boards CHECK.** The build fails with
  "set CONFIG_MAIN_STACK_SIZE >= N" (or the board's knob). Raising the knob
  automatically, through a two-step configure, is opt-in only.
- **sccache: bypass it for the measured crates** when measuring. A sccache
  hit restores the rlib but not the save-temps bitcode: 11 of 27 crates kept
  their `.bc`. The rlib's own embedded `.llvmbc` is the LTO pre-link module
  and is not exact: 174 of 1,977 frames differ. Tools and build scripts stay
  cached. The cost is about 2x on a warm build of those crates (5.4 s to
  11.2 s, nros-cpp). A bitcode store that would remove that cost is deferred
  to issue 2414.

### Delivery, per road

Every road builds the library with target-scoped `-C save-temps`, walks it,
then uses the floor.

| road | when the floor is known | use |
| --- | --- | --- |
| PX4 | the placer runs cargo before PX4's configure | the walk runs in the placer, `NROS_STACK_FLOOR_OPEN/_SPIN` go into the placed header, and `nros_px4_read_stack_floor` checks STACK_MAIN and the WQ stacks at configure |
| cmake / Corrosion (C/C++) | after the archive builds | FEED: a custom command writes `nros_stack_floor_generated.h`, and the stack-defining TUs depend on it |
| Zephyr west | after `librustapp.a` / `libnros`, before the link | CHECK against `.config`. This replaces the configure-time `NROS_RUST_MAIN_STACK_FLOOR` |
| cargo leaf (`nros build`) | after cargo | CHECK against the board's stated stack |

Waves:

1. W1: parse batching, a cache under the build dir, the allowance schema,
   strict mode, the sccache bypass.
2. W2: cmake.
3. W3: PX4.
4. W4: Zephyr.
5. W5: cargo leaf.
6. W6: a twin lane that asserts peak ≤ floor.

### Ruling (A): allowances per image, unmeasured never refused (2026-11-10)

Three nucleo-f767zi images measured on the twin showed that the static
walk's indirect charge is wrong in both directions:

| image | build | static OPEN | twin peak | allowance | main stack | floor |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| Rust action server | LTO | 22,564 | 30,256 | +7,692 | 32,768 | 33,288 |
| Rust talker | LTO | 23,220 | 28,888 | +5,668 | 32,768 | 31,784 |
| C talker | relwithdebinfo | 24,312 | 11,696 | **−12,616** | 16,384 | 12,872 |

The walk is 3 to 8 KB low on the Rust images. On the C talker it is 12.6 KB
high, because it charges 21 indirect sites the deepest zenoh-scoped target
(8,392 B) and the targets that actually run are shallow. Keyed per
(rmw, target, role), the action server's 7,692 B would have refused the Rust
talker (34,008 B) and the C talker (35,208 B), and both fit their stacks.
So the maintainer ruled:

1. Allowances are keyed per IMAGE SHAPE (`image`, target, role), and may be
   negative (allowance = peak − static). A measured image's floor is then
   roughly its peak + 10 %, and the static part still tracks direct-frame
   growth.
2. An unmeasured image WARNS and is never refused, even under CHECK.
   `--emit-defines` writes `NROS_STACK_FLOOR_<ROLE>_MEASURED 0` for it.
   Strict mode (CI) refuses only images that have a row.
3. There is no in-repo stack change. The generated nucleo action server's
   floor (33,288 B) is above the module's 32 KiB Rust default, and the image
   raises its own `CONFIG_MAIN_STACK_SIZE` through its `os_config` when W4's
   CHECK tells it to.

A follow-up worth measuring later: if allowances for one backend and target
cluster across shapes, a per-target default could replace per-image rows.
That is a measured decision, not one to take from three images.

### Re-measured after issue 2401 (2026-11-10)

#3481 lowered every nucleo static walk and moved no peak (table in issue
2402). The three rows now read: action server +8,476, Rust talker +6,676,
C talker −4,416. Floors are unchanged at 33,288 / 31,784 / 12,872 B.

Two things this run found:

- **An allowance goes stale in the unsafe direction.** When a frame on the
  walk shrinks and the row stays, the floor falls and the peak does not; the
  C talker's old row gave 3,848 B against 11,696. Nothing detects this yet.
  W6's twin lane is the detector: it re-measures peak and static together
  and fails when `bytes != peak - static`.
- **A Zephyr workspace patched before #3455 built Rust images without LTO,
  silently.** The shared 3.7 workspace had every hunk but the staticlib-only
  one; `.text` was 376,484 B against 316,116 with it. The configure-time
  refusal (`nros_check_zephyr_rust_cargo_patch`) checked the board-facts
  hunk only. It now checks each hunk whose absence a build would not
  notice, the staticlib-only one included, and names the missing ones.

### W2: the cmake road CHECKs the FreeRTOS carrier (2026-11-11)

These rulings are restated here because W2 relies on them:

- **CHECK by default, auto-raise opt-in** (ruled 2026-11-10). The build
  fails only when a MEASURED image's floor exceeds its stated stack, naming
  the knob. An unmeasured image warns (ruling (A)), and that includes a walk
  that was refused. Nothing lowers a stack: "exact when measured" was ruled
  out because it shrinks stacks.
- **Raise-only stays a separate wave, W2b.** It needs the floor before the
  carrier compiles, which is a pre-link edge. CHECK runs POST_BUILD and adds
  no edge.
- **The image key was PROVISIONAL (2026-11-11; superseded by key (c), ruled 2026-11-12, see W4):**
  `<bringup or entry name>@<board>`, for example
  `fc_node_bringup@mps2-an385-freertos`. A build knows that name; it never
  knows the `nros new` template the nucleo rows are keyed by. The
  alternative is (b), a key the user states per image in `system.toml`. The
  key is computed in one function, `_nros_stack_floor_image_key`, so either
  can be swapped in cheaply. The three nucleo rows keep their keys until W4
  wires Zephyr.

What W2 does, in `cmake/NanoRosStackFloor.cmake`:

| piece | how |
| --- | --- |
| Rust frames | `nros_stack_floor_cargo_env(<import>)` at every Corrosion import site merges `-Csave-temps` into the import's `RUSTFLAGS` and `-fstack-usage` into its `CFLAGS`/`CXXFLAGS`. It merges into what riscv64's helper already sets, because a second `RUSTFLAGS=` entry would replace that one. FreeRTOS only; other platforms keep their plain flags and their sccache hits. |
| C frames | `-fstack-usage` on every non-imported target in the executable's link closure |
| inputs | the link's own `-Wl,-Map`: the walker's new `--link-map` reads its `LOAD` lines, so the input list is exact under any generator. Toolchain libs outside the build dir are external leaves. |
| bitcode | `--bitcode-dir '<build>/cargo/*/<triple>/<profile>/deps'`, a glob, because the keyed cargo directories are named at build time. `--sysroot` adds the target's core, alloc and compiler_builtins rlibs. |
| check | POST_BUILD, `--check-stack OPEN=<app_stack_bytes>` from the root `app_task_entry`. `--stack-knob 'NROS_FREERTOS_APP_STACK_KB={kb}'` puts the remedy in the knob's own unit. |
| off | `-DNROS_STACK_FLOOR=OFF` skips all of it |

Measured on a `nros new --board mps2-an385-freertos --lang c --template
talker` image:

- OPEN static 13,828 B, unmeasured floor 15,216 B, against the
  65,536 B `app_stack_bytes`: holds. The carrier comment's boot-time peak for
  that role was 6,096 B, so the walk over-states here, as on nucleo
  (issue 2402).
- With a test allowance row, a 4,096 B stack fails the build with
  `NROS_FREERTOS_APP_STACK_KB=7`, and 65,536 passes. Without the row, the
  same 4,096 B only warns.

- C++ frames come from GCC's `-fcallgraph-info=su` `.ci` files, which name
  each function by its assembler (mangled) name. A `.su` line names a C++
  function by its signature, and a lambda or template instance does not match
  any demangled text. The generated message FFI crates (a cargo command
  Corrosion does not run) get the same flags, and their bitcode is globbed in.
- A `nros new ... --lang cpp --template action-server` image: OPEN static
  15,220 B, unmeasured floor 16,744 B. The carrier's measured C++
  action-server peak is 19,112 B, so the walk UNDER-states this image by
  about 3.9 KB, which is why an unmeasured floor is never enforced.

A FreeRTOS row is cheap to get: every carrier image prints `app task stack
peak` at boot.

### W4: Zephyr CHECKs the main stack (2026-11-12)

Same rulings as W2: CHECK by default, a MEASURED image fails, an unmeasured
one warns, and nothing lowers a stack. Two more, from the user (2026-11-12),
quoted:

> "go (c) and fail"

1. **The image key is (c).** `nros new` writes the template's key into the
   generated `system.toml`, `[image.<id>] stack_floor_image =
   "talker/rust/zenoh@nucleo-f767zi-zephyr"`, so the nucleo rows keep their
   template keys and a renamed project still matches. A row whose template or
   board no longer exists fails `--validate-table` loudly, and an image whose
   entity counts differ from the shape its row was measured on gets a loud
   warning. The key is decided in one function.
2. **A MEASURED image whose walk cannot run FAILS the build** (the behaviour
   #3556 shipped stays). An unmeasured one warns.

The configure-time 32 KiB floor for Rust images (issue 2359) stays, as
planned below.

| piece | how |
| --- | --- |
| Rust frames | `nros_stack_floor_zephyr_cargo_env` gives both Zephyr cargo commands (the C lane's `nros_cargo_build` and zephyr-lang-rust's, through `NROS_BOARD_FACTS_ENV`) `CARGO_TARGET_<TRIPLE>_RUSTFLAGS=-Csave-temps` and an empty `RUSTC_WRAPPER`. The target-scoped variable joins a config file's `[target.<triple>] rustflags` instead of replacing them, and keeps build scripts and proc-macros unflagged. The generated message FFI crates get the same rows. |
| C frames | `zephyr_compile_options(-fstack-usage, -fcallgraph-info=su)`: every Zephyr library and the app |
| inputs | `zephyr/zephyr.map`, which Zephyr copies from the final link's map before any extra post-build command runs |
| check | a `zephyr_final` post-build command (`extra_post_build_commands`), `--check-stack OPEN=<CONFIG_MAIN_STACK_SIZE>` from `rust_main` (a Rust entry) or `main` (C/C++), remedy `CONFIG_MAIN_STACK_SIZE=<floor>`. Zephyr runs those commands through the shell without VERBATIM, which would expand the bitcode globs, so the walk runs from a generated `nros/stack-floor/check.cmake`. |
| not checked | native_sim (`CONFIG_ARCH_POSIX`: a Zephyr thread runs on a host pthread's stack), a board with no nano-ros board facts, `NROS_STACK_FLOOR=OFF` |

How key (c) is carried:

| piece | how |
| --- | --- |
| spelling | `stack_floor_template_key` in `nros-cli-core` (`cmd/new_workspace.rs`): `<template>/<lang>/<rmw>@<board>`, from `Role::name` and `Language::as_str`. The only writer. |
| stamp | `nros new <ws> --board … --template …` writes it under the image's `[image.<id>]` |
| schema | `ImageBlock.stack_floor_image` (typed, `deny_unknown_fields`) and `LeafSystem.stack_floor_image` (the untyped reader both roads share); per image, never inherited from `[image_defaults]` |
| delivery | `nros dev image facts --cmake` emits `NROS_IMAGE_STACK_FLOOR_IMAGE`; the Zephyr check reads the facts file this configure already wrote for the image-agreement check |
| decision | `_nros_stack_floor_image_key`: the stated key, else `<bringup or entry>@<board>`. A hand-written application, or an image whose `system.toml` predates the stamp, keys by its bringup |
| row check | `--validate-table` (on the fast line): a `<template>/<lang>/<rmw>@<board>` row must name a template `Role::name` still has and a board with a `packages/boards/nros-board-<board>/nros-board.toml` |
| shape | each row has `[allowance.shape]`, the derived entity counts it was measured on (nodes, publishers, subscribers, queryables, callbacks, action clients). The Zephyr build passes its own from `NROS_BOARD_FACTS_ENV` as `--shape`; a difference prints `WARNING: image … is NOT the shape its OPEN allowance was measured on` naming each count. The allowance still applies. |

The FreeRTOS carrier (W2) does not read the stated key yet: no FreeRTOS image
has a row, and its configure does not run the image-facts query. It keys by
bringup until a FreeRTOS row exists (issue 2461).

The configure-time `NROS_RUST_MAIN_STACK_FLOOR` (32 KiB, issue 2359) STAYS.
An unmeasured image is never refused here, so removing it would put every
unmeasured Rust image back on the silent overflow 2359 found. It can retire
once measured rows cover the boards.

Measured on the three `nros new --board nucleo-f767zi-zephyr` projects, built
by `nros build` with the check:

| image | walk | allowance | floor | `CONFIG_MAIN_STACK_SIZE` | outcome |
| --- | ---: | ---: | ---: | ---: | --- |
| `nc` C talker | 16,112 | −4,416 | 12,872 | 16,384 | holds |
| `nt` Rust talker | 22,212 | +6,676 | 31,784 | 32,768 | holds |
| `na` Rust action server | 21,780 | +8,476 | 33,288 | 32,768 | **fails**: `Raise it: CONFIG_MAIN_STACK_SIZE=33288` |

Each walk equals the static figure the row was measured with, so the build
reproduces the hand measurement. The action server failing is ruling (A)(3):
the image raises its own stack through its `os_config` when the CHECK says so.

**So `nros new` raises it for the image.** A template whose recorded floor is
above the main stack the image would otherwise get writes `stack.conf`
(`CONFIG_MAIN_STACK_SIZE=<floor>`) into its bringup and names it in
`[image.<id>] os_config`. Both numbers are DERIVED, not hand-set:

- `template_main_stack` in `nros-cli-core` (`cmd/new_workspace.rs`) computes
  the floor from the row (`[allowance.measured] static` + `bytes`, by
  `floor_formula`'s arithmetic).
- It reads the default from the board package's conf, else from
  `zephyr/Kconfig`'s `configdefault MAIN_STACK_SIZE`.

A re-measured row moves what the next `nros new` writes. Nothing is written
when the default already holds, so nothing lowers a stack. The unit test
`every_measured_template_gets_a_main_stack_at_its_floor` asserts it for every
row. A fresh `nros new --template action-server --board nucleo-f767zi-zephyr`
now builds with 33,288 B against its 33,288 B floor; the talkers get no
`stack.conf`.

On `mps2-an385-zephyr` the walk is refused (`z_malloc`/`z_free`/`z_sleep_us`
have no frame there). Those images are unmeasured, so this only warns, but they
get no floor: issue 2460.

With `CONFIG_MAIN_STACK_SIZE=33288` in the `na` bringup's `os_config`, the
same image builds ("OPEN stack 33288 B holds the floor 33288 B (measured)").
A fresh `nros new nu ... --template talker` is the `nt` image under another
name: it builds with the UNMEASURED warning, floor 24,440 B (walk + margin,
no allowance) against 32,768 B.

Key (c), measured on fresh `nros new --board nucleo-f767zi-zephyr` projects
named unlike the measured ones (`xt`, `xa`, `xl`):

| project | stamped key | outcome |
| --- | --- | --- |
| `xt` Rust talker | `talker/rust/zenoh@…` | measured, holds (31,784 B against 32,768 B) |
| `xa` Rust action server | `action-server/rust/zenoh@…` | measured, **fails**: `Raise it: CONFIG_MAIN_STACK_SIZE=33288` |
| `xl` Rust listener, key edited to the talker's | `talker/rust/zenoh@…` | builds with `WARNING: … is NOT the shape its OPEN allowance was measured on (NROS_MAX_PUBLISHERS measured 1, this image 0; NROS_MAX_SUBSCRIBERS measured 0, this image 1)` |

### W6: the twin lane (2026-11-13)

`just zephyr stack-floor-twin` (`scripts/stack-floor-twin.py`) re-measures
every row whose key is a template key on a Renode-twin board. It builds the
template with the thread analyzer and runs it in Renode. Then it compares the
main thread's peak with the build's own static walk, which the walker now
writes to `nros/stack-floor/report.json`. It fails when the peak is above
the floor, or when `bytes != peak - static`.

Its first run confirmed both Rust rows exact. It found the C talker's peak
180 B above its row (11,876 against 11,696 B) with the walk unchanged at
16,112 B: the unsafe drift this wave was for. The row is now −4,236 B, floor
13,064 B. The verdict table and the item status are in
[phase-557](../roadmap/phase-557-stack-floor-measured-frames.md) W6.

### W2b: an opted-in image is raised to its floor (2026-11-13)

`[image.<id>] stack_floor = "raise"` turns a measured image's CHECK failure
into a second build at the floor, on both FreeRTOS roads. A pre-link edge
cannot exist, because heap_4 sizes the heap at configure.

- **Never stored, never lowers.** The raise is derived again by every build
  and never lowers a stack.
- **Unmeasured images.** An unmeasured image still only warns.
- **Pinning.** The build prints the value it used. On the cargo road it also
  prints the `[image.<id>.knobs.freertos] app_stack_kb` line that pins it. On
  the cmake road there is no such line yet (issue 2496).

The detail is in
[phase-557](../roadmap/phase-557-stack-floor-measured-frames.md) W2b.

### W5: the cargo leaf CHECKs the board's stated stack (2026-11-13)

Same rulings: CHECK by default, a MEASURED image fails, an unmeasured one
warns, and nothing lowers a stack. `nros build` walks a cargo image after it
links.

- **Scope.** Only FreeRTOS boards state a stack
  on the cargo road today. ThreadX, NuttX and bare metal print "not checked:
  the board states no stack" and are phase-557 W5b. Bare metal has no
  stated stack at all (issue 2494).
- **The stated number.** The board's build script resolves the knob, so it
  writes the number. `nros build` finds the file through cargo's own
  `build-script-executed` message for the image.
- **The walker's three new inputs.** A `~text` root for the generic Rust
  app-task entry; `rust-lld` maps; and newlib frames from the linked image's
  CFI.
- **Measured** on the FreeRTOS Rust talker: static walk 23,720 B, unmeasured
  floor 26,096 B against 128 KiB. With a test allowance row, a 16 KiB stack
  fails with "Raise it: NROS_FREERTOS_APP_STACK_KB=26".

The detail is in
[phase-557](../roadmap/phase-557-stack-floor-measured-frames.md) W5.
