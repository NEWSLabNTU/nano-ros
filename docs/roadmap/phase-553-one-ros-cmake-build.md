# Phase 553 — One C/C++ build method: a ROS 2 CMake package on every board

**Status (2026-11-12). IN PROGRESS: W1 done; W2 to W6 planned.**
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

- **Status:** DONE (2026-11-12, #3637)

The bare-metal platform crates exported C library symbols behind default
features (`libc-stubs`: `strlen`, `memcpy`, `snprintf`, `vsnprintf`, …;
`libc-heap`: `malloc`, `free`, `realloc`, `calloc`), and many images take the
board crate with `default-features = false` yet still rely on them, so
moving the stubs behind the board's defaults would have broken their links.
Cargo features are additive, so the opt-out is its own feature: `c-libc` on
`nros-board-mps2-an385` (forwarded to `nros-platform-mps2-an385` and
`nros-baremetal-common`) removes every stub export, whoever turned
`libc-stubs`/`libc-heap` on. A Rust-rooted image is unchanged (it never sets
`c-libc`). newlib (nano) owns libc in a C-rooted image.

Heap layout, one owner per heap and no overlap: the Rust allocator's arena is
a static array in `.bss` (`nros_platform_mps2_an385::memory::HEAP`, 132,816
bytes in the probe); newlib's `_sbrk` grows from `end` = `__sheap`, the end
of `.bss`, upward toward the stack. The main stack bounds that layout too. It
is unbounded today and grows down from the top of RAM toward the `_sbrk` heap
with no guard; see
[issue 2494](../issues/2494-mps2-baremetal-stack-unbounded.md).

Measured on the RFC-0130 D4 probe with the board's `c-libc` on, against a
zenoh router:
- the C component prints `Publishing: 'Hello World: 1'`, `… 2`, `… 3`, and
  `RCLC_LOG_INFO` prints `c_talker logging seq=1` (it printed `''` and `%d`
  with the stubs exported);
- the link map takes `snprintf`, `vsnprintf`, `malloc`, `free` and `_sbrk`
  from `libc_nano.a`, and the staticlib defines none of them (`nm`);
  `strlen`/`memcpy` resolve to Rust's `compiler_builtins`, its own weak
  fallbacks rather than our stubs;
- no default changed, so every cargo-rooted image keeps its symbols; the push
  gate's embedded builds cover them.

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
