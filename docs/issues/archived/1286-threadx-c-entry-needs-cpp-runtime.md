---
id: 1286
title: "A C-only ThreadX entry still needs the C++ runtime: ThreadX is the one RTOS
  the C-ABI board surface skips"
status: resolved
type: limitation
area: cli, codegen, threadx
severity: low
resolved_in: "fix(#1286): ThreadX takes the shared C runner, so a C ThreadX entry is pure C"
related: [issue-1283, issue-1285, issue-0582, issue-0245, phase-432, rfc-0091]
---

## What happens

Phase-432 W3.1 gave FreeRTOS, Zephyr and NuttX a C runner
(`nros_board_rtos_run_components`, one TU for all three), so a C entry on those
boards is a pure C image. ThreadX was left out: `BoardFamily::Threadx`'s
`has_c_run_components()` is false, so `entry-pack` routes a ThreadX C entry to
the C++ pack. A certified-C, no-C++-runtime consumer cannot target ThreadX.

RFC-0091 §11 records the reason as "nothing to copy". That is no longer true:
the shared runner exists, and ThreadX's C++ `run_components` is line for line
FreeRTOS's (network wait, init, setup, spin loop, shutdown). The boot path fits
the C pack's `app` shape: the board's weak `main` calls `tx_kernel_enter()`, and
the app thread calls the weak `app_main()` that `NROS_APP_MAIN_REGISTER_VOID()`
defines. No per-tick yield is needed (`entry_tick_yield` acts only on Zephyr,
and `spin_once(10)` blocks).

## Measured (2026-09-11, threadx-linux, zenoh, then reverted)

The CLI predicate was flipped and `workspace-c-threadx-linux` was built
(`NROS_FIXTURE_ID=workspace-c-threadx-linux bash
scripts/build/workspace-fixtures-build.sh threadx-linux c`):

- `entry-pack` answered `pack=c`. The entry and `nros_rtos_run_components.c`
  compiled as C: 0 CXX objects, linked with `cc`.
- `libstdc++.so.6` no longer NEEDED. `__cxa|_ZSt|_ZN9__gnu_cxx` matches went
  7 -> 4, and the remaining 4 are glibc's `__cxa_finalize` /
  `__cxa_thread_atexit_impl`.
- `nros_board_rtos_run_components`, `nros_app_main` and `app_main` are GLOBAL.
- Not tested: runtime, the rv-virt board, Rust ThreadX images.

## Why it is not a one-line flip

TIERS. There is no `nros_board_threadx_run_tiers`. The C++ pack handles a tiered
ThreadX plan on a single executor with sched-contexts, and the C pack has no
sched-context path (issue 1283). Routing cannot depend on the plan either:
CMake asks `nros codegen entry-pack --lang --board` for the file extension
before any plan exists (`NanoRosEntry.cmake`, around line 975).

So the order is: issue 1283 gives the C pack sched-contexts, then ThreadX routes
to C unconditionally, and a tiered ThreadX plan takes the sched-context path,
exactly as in C++.

## Risk to keep in view

The runner's fallback storage size is 81920. The per-build
`nros_cpp_config_generated.h` for this image says 90392. If that header is ever
not visible to a ThreadX compile of the runner, that is issue 0245's
heap-corruption shape. Assert the header is on the ThreadX include path rather
than trusting the fallback.

## Acceptance

`workspace-c-threadx-linux` builds as a C image with 0 C++ runtime NEEDED
entries, on both threadx-linux and rv-virt-threadx, and a tiered ThreadX C plan's
golden emits the sched-context path.

## Resolution

Fixed, on top of issue 1283 (the C pack's sched-context path) and issue 1285
(`BoardFamily::c_abi_runners`).

**Routing.** `BoardFamily::Threadx.c_abi_runners()` is now
`run_components = nros_board_rtos_run_components`, `run_tiers = None`. So
`has_c_run_components()` is true for every family, and a ThreadX `--lang c`
entry renders through the C pack. A multi-tier ThreadX plan gets
`ExecutorShape::SchedContexts` from `Plan::executor_shape`, because
`board_has_run_tiers` reads the `None` half. Both packs take that branch.
`both_entry_packs_take_the_plans_executor_branch` now includes ThreadX
(20 rows, was 16). Its ThreadX multi-tier row expects `SchedContexts`, and
the expectation is written out rather than derived from the predicate it
checks.

**Boot shape, read in the board C rather than assumed.** threadx-linux:
`startup.c`'s weak `main` calls `tx_kernel_enter()`.
`threadx_hooks.c::tx_application_define` creates the app thread, and that
thread calls the weak `app_main` off RISC-V. rv-virt: `startup.c::main`
registers `app_main` explicitly (`nros_threadx_set_app_main(app_main)`),
because a weak-undefined `app_main` cannot be reached with
`R_RISCV_PCREL_HI20`. Both reach the `app_main` that the C pack's
`NROS_APP_MAIN_REGISTER_VOID()` defines.

**CMake, and where the runner compiles.** The shared TU goes into the APP
target on both boards. threadx-linux: `THREADX_APP_DEFINE_SOURCE`. rv-virt:
`THREADX_STARTUP_SOURCE`, and deliberately NOT `_glue_srcs`:

- `threadx_glue` sees no nros-cpp include path. Its `threadx_hooks.c`
  compile has no nros-cpp or nros-c `-I`, measured in `compile_commands.json`.
  The runner there would silently take its 81920-byte fallback.
- It is an archive after `libnros_cpp.a`, so the runner's `nros_cpp_*`
  references would be unresolved.

In the app target the per-build mirror dir comes BEFORE the in-tree stub on
both boards, so a missing header is the stub's `#error`, never the fallback.
That is the existing mechanism, so nothing new was added.

The cargo lane is unchanged: `nros-board-threadx-linux`'s glue is
`+whole-archive` (issue 0582), and the runner there would pull `nros_cpp_*`
into every Rust image.

**The storage risk was real.** The per-build `NROS_CPP_EXECUTOR_STORAGE_SIZE`
is 90424 on BOTH boards (32 above this issue's 90392: issue 1172 grew the
executor). That is 8504 bytes over the fallback. Which header each runner
compile used, from its own depfile:
- threadx-linux `nros_rtos_run_components.c.o.d` names
  `…/cmake/nano_ros/packages/api/nros-cpp/include/nros/nros_cpp_config_generated.h`
  (90424).
- rv-virt `ninja -t deps` (VALID) names
  `…/nano_ros/packages/api/nros-cpp/include/nros/nros_cpp_config_generated.h`
  (90424).

**Build evidence (2026-09-11).**

threadx-linux, zenoh:
`NROS_FIXTURE_ID=workspace-c-threadx-linux bash scripts/build/workspace-fixtures-build.sh threadx-linux c`
- `nros codegen entry-pack --lang c --board threadx-linux` answers `pack=c`,
  `routed=0`. The build log has no "rendered by the C++ pack" line.
- The entry TU is `threadx_entry_nros_main_generated.c`, with 0 C++ objects
  in the build.
- `threadx_entry` (built 11:43:12): `readelf -d` NEEDED is `libgcc_s.so.1`,
  `libc.so.6` and `ld-linux-x86-64.so.2`, with no `libstdc++.so.6`. The only
  `__cxa|_ZSt|_ZN9__gnu_cxx` undefined symbols are glibc's weak
  `__cxa_finalize` and `__cxa_thread_atexit_impl`.
- `nm`: `nros_board_rtos_run_components`, `nros_app_main` and `app_main` are
  all `T`.

rv-virt-threadx, zenoh, xPack `riscv-none-elf-gcc` 14.2:
- There is no rv-virt C workspace row in `examples/fixtures.toml`, so no
  workspace build exists to run. `examples/rv-virt-threadx/c/talker` (a plain
  `nano_ros_add_executable` image) was configured and built with the lane's
  own `-D` set.
- The runner compiled in the app target against the per-build header.
- `c_talker` linked. It carries neither `nros_board_rtos_run_components` nor
  an undefined `nros_cpp_*`: every rv-virt app links with `--gc-sections`,
  and an unreferenced section's undefined references are not reported
  (checked with a two-line probe: rc 0 with gc, an error without).
- NOT measured: a C WORKSPACE entry on rv-virt, which needs a fixture row
  first.

**Goldens.**
- `c_threadx_one`: was the refusal text, now a C entry calling
  `nros_board_rtos_run_components` in the `app` shape.
- `c_threadx_tiers` (new): the three sched calls plus the same runner.

**Runtime (threadx-linux).** PASSED, solo. The command was
`cargo test -p nros-tests --test entry_e2e entry_matrix`, and it printed
`entry_matrix: 1 ran, 15 skipped, 0 failed`. The cell that ran is
`threadx-linux/c/entry_pubsub`.

That cell boots the pure-C `threadx_entry` built above. A native C listener
observer must log `INT32_LISTENER_LOG_PREFIX` at least 3 times within 20 s,
so the shared runner's init, setup, spin and publish all ran on ThreadX. Its
observer is the `workspace-c-native-robot2` row's `native_robot2_entry`,
built for this run.

The 15 skips are cells whose fixtures this worktree did not build: Zephyr,
FreeRTOS, NuttX, and the threadx-linux C++ and mixed cells. rv-virt has no C
workspace cell, so its runtime is untested.

## Follow-up (2026-09-11): the rv-virt C workspace entry

The gap above is closed. A C workspace row now exists for rv-virt-threadx, and it
has been built and run.

**Declared.**
- `examples/workspaces/c` `system.toml`: `[image.riscv_threadx]` with
  `board = "rv-virt-threadx"` (the `riscv_nuttx` spelling), plus its
  `board_config`.
- `examples/fixtures.toml`: `workspace-c-threadx-riscv64`, a migrated image row.
  Its locator is `tcp/10.0.2.2:9530`, which is
  `alloc::port_of(ThreadxRiscv64, C, EntryPubsub)`. Its `build_subdir` is
  `build/threadx-riscv64-zenoh-rv-virt-threadx/cmake`, the name `nros build`'s
  `cmake_coordinate` gives this image.
- `matrix::CELLS`: `cell(ThreadxRiscv64, C, Zenoh, EntryPubsub, Workspace, Runtime)`,
  claimed by `entry_e2e` in `w1_consumer_of`. `entry_e2e` gained
  `Boot::ThreadxRiscv64`: `QemuProcess::start_riscv64_virt(entry, 0)`, user-mode
  slirp, router bound on 0.0.0.0.
- `just threadx_riscv64 build-fixture-extras` builds the row.

**Built** with the repo's builder, xPack `riscv-none-elf-gcc` 14.2.0, newlib:
`NROS_FIXTURE_ID=workspace-c-threadx-riscv64 scripts/build/workspace-fixtures-build.sh threadx-riscv64 c`
- `nros codegen entry-pack --lang c --board rv-virt-threadx` answers `pack=c`,
  `routed=0`. The build log has no "C++ pack" line.
- The entry TU is `riscv_threadx_entry_nros_main_generated.c`. The build dir
  holds 0 C++ objects, and the link line names no `stdc++`, `supc++` or `g++`.
- `riscv_threadx_entry` linked, a static RV64 ELF.
  `riscv-none-elf-nm` shows `app_main`, `nros_app_main` and
  `nros_board_rtos_run_components` all as `T`. There are 0 matches for
  `__cxa_|_ZSt|_ZN9__gnu_cxx|__gxx_personality`, and no undefined symbols.
- The runner (`CMakeFiles/riscv_threadx_entry.dir/…/nros_rtos_run_components.c.obj`)
  compiled in the app target, as `THREADX_STARTUP_SOURCE` intends. Its depfile
  names `…/cmake/nano_ros/packages/api/nros-cpp/include/nros/nros_cpp_config_generated.h`,
  where `NROS_CPP_EXECUTOR_STORAGE_SIZE` is 90424, not the 81920 fallback. In
  `flags.make` that mirror's `-I` comes before the in-tree stub's
  `packages/api/nros-cpp/include`, so a missing header is the stub's `#error`.

**Runtime.** PASSED, solo:
`cargo test -p nros-tests --test entry_e2e entry_matrix` printed
`entry_matrix: 1 ran, 16 skipped, 0 failed (of 17 cells)`. The cell that ran is
`threadx-riscv64/c/entry_pubsub`. The pure-C entry boots in QEMU riscv64 virt
(NetX Duo over virtio-net, 10.0.2.40) and publishes `/chatter` through the slirp
gateway. The native C listener (`native_robot2_entry`, built for this run) logged
`INT32_LISTENER_LOG_PREFIX` at least 3 times within 90 s. The 16 skips are
fixtures this worktree did not build: Zephyr, FreeRTOS, NuttX, and threadx-linux.

All of this was measured twice. The first pass ran on the original base with
this fix cherry-picked. The second ran after rebasing onto the `main` that merged
it, which had also moved the CLI, `nros-entry-lower` and the rmw cffi bindings.
For the second pass the CLI was rebuilt first, then both fixtures, and the image
was built at 16:03:33, after the rebased HEAD (15:48:44). Every fact above
reproduced unchanged: the same symbols at the same addresses, 90424, and the
same include order. The cell passed again: 1 ran, 16 skipped, 0 failed, in
12.7 s (80 s on the first, cold run).
