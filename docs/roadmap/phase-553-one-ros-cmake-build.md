# Phase 553 — One C/C++ build method: a ROS 2 CMake package on every board

**Status (2026-11-12). PLANNED: the direction is ruled, no item has started.**
Work for [RFC-0130](../design/0130-one-ros-cmake-build.md). Maintainer ruling
2026-11-12, "Accept the unified ROS-CMake build": a C/C++ node package is an
ordinary ROS 2 `ament_cmake(_auto)`-style package on every board, with no
Cargo or board boilerplate on the user's side. On a board whose startup is
Rust (bare-metal mps2-an385), CMake owns the final link, and the runtime plus
the board's startup arrive as a Corrosion staticlib. RFC-0130 D7's NuttX
questions are decided (2026-11-12) and need no item here: NuttX keeps its
current CMake road, and its guest make road is tracked by phase-550. Each item is claimed with
`just claim phase-553-wN` before it starts.

---

## Why

A C/C++ user on bare metal today writes a cargo crate (`Cargo.toml`,
`build.rs`, a Rust `main.rs`) that compiles their C, while on every RTOS the
same node is a CMake package. Two methods for one job, and the bare-metal one
is not ROS-shaped. RFC-0130 D4 measured that the CMake-rooted road works on
mps2-an385: a C `main` linked by the C toolchain against a Rust staticlib that
carries cortex-m-rt's startup boots under QEMU, and a C component publishes
through the shared C runner.

## Work items

### W1 — One libc and heap owner on a C-rooted image

- **Status:** OPEN

The bare-metal platform crates gate their C library exports behind default
features (`libc-stubs`: `strlen`, `memcpy`, `snprintf`, `vsnprintf`, …;
`libc-heap`: `malloc`, `free`, `realloc`, `calloc`), but two dependents force
them on by taking `nros-platform-mps2-an385` with its defaults:
`nros-board-mps2-an385` and `nros-platform`. Take it with
`default-features = false` in both and forward the two features through the
board crate's defaults, so a cargo-rooted image keeps them and the C-rooted
staticlib turns them off. newlib (nano) then owns libc. Decide and state one
heap layout: newlib's `_sbrk` region and the Rust allocator's arena must not
overlap.

Acceptance:
- The RFC-0130 D4 probe's C component prints `Publishing: 'Hello World: N'`
  and `RCLC_LOG_INFO` prints its expanded arguments (`seq=N`, not `%d`).
- The probe's map shows `snprintf`/`vsnprintf`/`malloc` from newlib, none
  from a Rust crate.
- Every existing cargo-rooted mps2-an385 image still links: the
  `baremetal-*` workspace rows and the leaf rows build (no feature lost).

### W2 — C++ static constructors run at startup

- **Status:** OPEN

cortex-m-rt runs no `.init_array`. A C-rooted image calls
`__libc_init_array` before `main` (cortex-m-rt's `pre_init`, or the generated
entry, whichever the measurement favours).

Acceptance: a C++ component with a namespace-scope object whose constructor
sets a flag reports the flag set at its first callback, on mps2-an385 under
QEMU.

### W3 — No `-lgcc` on the C-rooted link

- **Status:** BLOCKED (needs W4: the link line it constrains is the one W4's road generates)

Measured in RFC-0130 D4: libgcc drags ARM unwind code needing
`__exidx_start`/`__exidx_end` and `abort`. The C-rooted link line omits it,
and a C++ component builds with `-fno-exceptions -fno-rtti`.

Acceptance: the generated link line for a C and a C++ image carries no
`-lgcc` and links with no undefined symbol; a check fails if it reappears.

### W4 — The CMake-rooted road on mps2-an385

- **Status:** OPEN

- The board's C-ABI start (`nros_board_<board>_start(app)`: log sinks,
  `init_hardware`, network, then `app`), in the board crate or a thin
  board-seam staticlib so `nros-c` never names a board (RFC-0064).
- The Corrosion staticlib carries the board, cortex-m-rt and the PAC, and
  cmake receives the three link-script directories (cortex-m-rt `link.x`,
  the board's `memory.x`, the PAC's `device.x`).
- A `nros-entry-lower` family row for the board, runner
  `nros_board_rtos_run_components_in`, so the C/C++ entry pack renders it;
  executor storage sized from the build, never by hand.
- `nros build` keeps a C/C++ image on this board on the cmake road, and
  `nros new --lang c|cpp` stops refusing bare metal.

Acceptance:
- A C and a C++ talker written as ordinary component packages (no Cargo, no
  board name) build with `nros build` for `mps2-an385-baremetal` and publish
  to a ROS 2 subscriber through a zenoh router.
- The same two packages, unchanged, build for an RTOS board.
- `just check dep-chain` passes; the stack floor and `mem-report` are
  measured on the new images and recorded here.

### W5 — Migrate the bare-metal C/C++ talkers

- **Status:** BLOCKED (needs W4)

`examples/mps2-an385-baremetal/{c,cpp}/talker` become one-package workspaces
whose package is an ordinary component, booted by the generated entry.

Acceptance: their fixture rows are workspace rows on the cmake road, and
their runtime cells pass.

### W6 — Retire the cargo-built C/C++ bare-metal workspaces

- **Status:** BLOCKED (needs W4). Owner: to be assigned; this is not claimed with
W1 to W5 (the session that migrated these workspaces is the expected owner).

The ten `examples/mps2-an385-baremetal/{c,cpp}/{listener,service-*,action-*}`
workspaces whose package is a hand-written cargo crate move to ordinary
component packages, and `nros-cc-flags`' leaf-only modules lose their last
consumers (measured before deleting).

Acceptance: no tracked `Cargo.toml` + `build.rs` compiles C or C++ for an
example; `test_qemu_baremetal_capi_e2e` passes against the new rows.

## Not in scope

- NuttX (RFC-0130 D7, decided): this phase changes no NuttX road; the guest
  make road's work is phase-550's.
- The cargo-entry design (a generated cargo entry compiling component sources
  by convention): superseded by RFC-0130 and never built.
