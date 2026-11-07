# Phase 548 — Rust, C and C++ on every board in the user book

**Status (2026-11-07). NOT STARTED.** User ruling (2026-11-07): every board
in the user book supports all three languages, and the book keeps the Rust,
C and C++ tabs on every page. A language that does not work on a board yet is
a gap to close, not a tab to drop. The manager session assigns each work item;
claim it with `just claim phase-548-wN` before starting. The items are
independent unless a line says otherwise.

**What "supports" means** (user clarification, 2026-11-07): the user writes
only that language. A C or C++ component, or a Rust package, is scaffolded
with `nros new`, built with `nros build` and run with `nros run`. Glue the
build generates in another language is fine, for example the generated Rust
entry that owns reset on bare metal.

## Measurement

Measured on main 237178f538, with the in-tree CLI and ROS 2 Humble. Each cell
is `nros new <ws> --board <b> --template talker --lang <l> --rmw <rmw>`, then
`nros build`, then `nros run <ws>_node --expect 'Publishing:'`. Scaffolding
works in all 36 cells. The per-cell logs from that run are not kept in the
repository; W8 re-measures every cell.

| Board (RMW) | Rust | C | C++ |
|---|---|---|---|
| linux (cyclonedds) | ok | ok | ok |
| freertos-posix (zenoh) | 2342 | 2342 | 2342 |
| freertos-posix (cyclonedds) | **W1** | ok | ok |
| mps2-an385 bare metal (zenoh), Tutorial 1 to 10 | ok | 531 | 531 |
| mps2-an385 FreeRTOS (zenoh) | ok | ok | ok |
| mps2-an385 Zephyr (zenoh) | 3268 | ok | **W2** |
| Zephyr native_sim (zenoh) | 3268 | ok | **W2** |
| NuttX qemu-armv7a (zenoh) | ok | ok | **W3** |
| NuttX rv-virt (zenoh) | ok | ok | ok |
| ThreadX Linux (zenoh) | ok | ok | ok |
| ThreadX qemu riscv64 (zenoh) | ok | ok | **W4** |
| nucleo-f767zi Zephyr (zenoh) | **W7** | **W7** | **W7** |

Gaps already owned outside this phase. This phase does not repeat them; W8
re-measures their cells once they land:

- **2342**: zenoh-pico has no FreeRTOS port for a host-owned network stack
  (`lwipopts.h: No such file or directory`). Fixed by #3273.
- **531**: C and C++ on mps2 bare metal. Today it fails with
  `tools/setup.sh: unknown platform 'bare_metal'`, from
  `builder/cmake_root.rs:233`. Fixed by phase-531 W6 batch 2: a CMake C
  component, with the board routed to a generated Rust entry that owns reset.
  The C++ cell also needs the GCC floor that W3 adds for NuttX.
- **3268**: Rust on Zephyr fails with `nros::main!: pkg '<bringup>' not found
  … Known pkgs: []`, because the west entry got no `NROS_WORKSPACE_ROOT`.
  Fixed by #3268.
- **The C++ scaffold**: the scaffold's metadata probe calls `ok()`, `error_what()`
  and `error_code()`, which the scaffolded class does not have. The build goes
  on without the probe's facts, so no cell flips on it. The C++ scaffold
  fix already in flight covers it.

## Work items

### W1 — Rust on the FreeRTOS POSIX board

**Code area:** `packages/boards/nros-board-freertos-posix/`, the board-crate
resolution for a generated entry in `nros-cli-core`
(`orchestration/backend_link.rs`, `codegen/entry/emit_rust.rs`), and the
cyclonedds cargo bridge.

- Measured: the generated
  `build/freertos-zenoh/freertos_entry/Cargo.toml` depends on
  `nros-board-mps2-an385-freertos`, which is the lwIP/ARM board, not the host
  one.
- Measured: with `--rmw cyclonedds` the build refuses: `backend cyclonedds
  provides only a cmake target; a Rust-rooted build needs a sys_crate bridge …
  board crate nros-board-mps2-an385-freertos does not link it`
  (`orchestration/backend_link.rs:176`).
- Cause, as read and not yet traced end to end:
  `nros-board-freertos-posix` has no Rust crate, and its `nros-board.toml`
  names no `board_crate`. The image key `freertos` then matches the mps2
  board's alias (`names = ["freertos", …]` in
  `packages/boards/nros-board-mps2-an385-freertos/nros-board.toml:4`). The
  test at `codegen/entry/emit_rust.rs:530` calls the POSIX board "C-only".
- Work:
  - give the board a Rust crate with the POSIX port and the host network
    stack;
  - make an image whose board crate is unresolved refuse, never fall back to
    another board's alias;
  - give cyclonedds a cargo bridge (or a `sys_crate`) that this board links.

**Acceptance:** the freertos-posix Rust cell is green on cyclonedds, and on
zenoh once 2342 lands. An image whose board names no crate is refused with
the board's name.

### W2 — C++ on Zephyr compiles as C++14

**Code area:** the Zephyr entry's `prj.conf` writer in `nros-cli-core`
(`codegen/entry`, the Zephyr pack).

- Measured: `rclcpp/qos.hpp:292:79: error: 'constexpr' constructor does not
  have empty body`. The failing object,
  `zephyr_entry_nros_main_generated.cpp.obj`, is compiled with `-std=c++11`.
- Cause: the generated `build/zephyr-zenoh/zephyr_entry/prj.conf` sets only
  `CONFIG_NROS=y`. `codegen/entry/packs/entry/cpp/entry.cpp.jinja:21`
  assumes C++14.
- Work: when the image holds a C++ component, write `CONFIG_CPP=y` and
  `CONFIG_STD_CPP14=y` (or the standard the C++ API requires), plus a unit
  test that renders the `prj.conf` for a C++ image.

**Acceptance:** the C++ cells of mps2-an385 Zephyr and Zephyr native_sim are
green.

- Found while measuring W2, outside it: issue 2353 (`nros run` on
  mps2-an385 Zephyr finds no west workspace without `ZEPHYR_BASE` and starts
  no router for the board's `10.0.2.2` locator) and issue 2354 (a generated
  Rust Zephyr image on Cyclone DDS probably needs `CONFIG_CPP`; not measured).

### W3 — The NuttX cross compiler is the pinned one, with a GCC floor

**Code area:** `packages/boards/nros-board-common/src/nuttx_ffi_build.rs`.

- Measured: `NUTTX_CROSS` defaults to a bare `arm-none-eabi-gcc` (line 41),
  so the build uses whatever is on PATH. With system GCC 10.3, it fails with
  `nuttx_entry_nros_main_generated.cpp:47:1: error: C99 designator
  'node_name' outside aggregate initializer`, from the designators in
  `codegen/entry/packs/entry/shared/boot_config.jinja`. With the store's
  13.2-nros5 it builds and runs.
- Work:
  - resolve the store's pinned toolchain the way the FreeRTOS road does;
  - refuse GCC below 11 with a message, as `NanoRosEntry.cmake:480` does on
    FreeRTOS.

**Acceptance:**
- the NuttX qemu-armv7a C++ cell is green on a host whose system
  `arm-none-eabi-gcc` is older than 11;
- with the pin missing, the build refuses with the pin's name, never falls
  back silently.

**Done (2026-11-07).** `nros-board-common::nuttx_toolchain::resolve()` is the
one answer for all four NuttX build-script sites (`nuttx_ffi_build`,
`nuttx_image_link`, both `nuttx_platform_build` passes), and the FFI build's
cc-rs compiles now set `.compiler()` for C and C++ from it. A bare tool name
(or the unset default) resolves `<store>/sdk/<tool>/<pin>/bin/<tool>`; a path
in `NUTTX_CROSS` is used as given. A C++ image below GCC 11 is refused before
the first compile. Measured on a host whose first `arm-none-eabi-g++` on PATH
is system 10.3.1:
- `nros new nxcpp --board qemu-armv7a-nuttx --template talker --lang cpp
  --rmw zenoh`, `nros build`, `nros run nxcpp_node --expect 'Publishing:'`:
  matched after 1.1 s (with a zenoh router on the host's 7447);
- with `NROS_HOME` pointing at a store that lacks `13.2-nros5`, a fresh C image
  refuses: `NuttX cross compiler: the pinned arm-none-eabi-gcc 13.2-nros5 is
  not provisioned (expected …)`.

### W4 — C++ on ThreadX qemu riscv64 publishes

**Code area:** `packages/boards/nros-board-threadx-qemu-riscv64/` (the C++
entry's start-up path on this board).

- The boot fault is fixed by #3278 (phase-535 W5): the newlib
  `__retarget_lock_init` shim read `*lock` instead of allocating.
- Re-measured on #3278's head (76e4ff51de, 2026-11-07), with a router on
  7447: the C++ talker boots with no fault, but the console prints
  `[app_thread] Calling c_app_main (FFI)...` and then
  `[app_thread] c_app_main returned` at once. It prints no error and
  publishes nothing. The C talker on the same board, with the same router,
  prints `Publishing: 'Hello World: 1'` onward. The C++ talker on ThreadX
  Linux publishes.
- Work: find why the C++ entry returns. If it is an error path, it must print
  its reason on the board's console. Then fix it.

**Acceptance:** the C++ cell is green.

### W4b — A board fault or an early return ends the QEMU run

**Code area:** the ThreadX qemu riscv64 board's trap handler and app thread
(`packages/boards/nros-board-threadx-qemu-riscv64/`), and how `nros run`
learns that the emulator stopped.

- Measured: after the boot fault above, and after `c_app_main returned`, QEMU
  keeps running until `nros run --timeout` expires (300 s). The run then
  reports a timeout, not the fault.
- Work: on a trap, and when `c_app_main` returns, exit QEMU with a failing
  status (semihosting or the virt test device). `nros run` then reports the
  status at once.

**Acceptance:** a deliberate fault, and a `c_app_main` that returns, each end
the run with a non-zero status within seconds.

### W5 — `nros setup` outside a checkout reads the current index

**Code area:** `nros-cli-core` `cmd/setup.rs` (`resolve_index`).

- Measured: outside a checkout, `nros setup` read a 26-day-old
  `~/.nros/fetch/nros-sdk-index.toml`. `nros setup mps2-an385-zephyr` and
  `nros setup nucleo-f767zi-zephyr` answered "unknown board", though both are
  in the current index. `nros run` then refused QEMU 11.0.0-nros7 and asked
  for nros8.
- This is the same class as archived issues 2184 and 2257.
- Work:
  - make setup, build and run read one index;
  - make a cached index older than the CLI's own refresh, or say so and name
    the remedy.

**Acceptance:** a test that seeds a stale fetched index and asserts that
`nros setup <board>` resolves a board present only in the current one.

### W6 — `nros build` reconfigures after a toolchain change — DONE

**Code area:** `cmake/toolchain/NanoRosCrossToolchain.cmake:363` and the
cmake call in `nros build`.

- Measured: after the pinned GCC is installed, `nros build` stops with "build
  directory is already configured with a DIFFERENT C compiler". The user must
  delete the build directory by hand.
- Done: the toolchain file's refusal also writes
  `<build>/.nros-toolchain-changed` (language, the compiler the tree has, the
  one that resolves now, and where from), and a configure that gets past the
  check removes any stale one. A file, because the configure's output goes
  straight to the terminal, and a message's wording is not an interface.
- `nros build`'s cmake road answers the marker once
  (`reconfigure_after_toolchain_change`, cmd/build.rs). It removes that
  directory's `CMakeCache.txt` and `CMakeFiles/`, which is what
  `cmake --fresh` does, without depending on a cmake new enough to have it.
  Then it configures again and prints which compiler changed to which.
  Build outputs stay. This is the one state the toolchain file's own comment
  names as unrecoverable in place, so it is not the `rm -rf` antipattern.
- A bare `cmake` keeps the refusal, with a line saying `nros build` handles
  it. The west road is untouched (agreed with phase-530, whose W7 skips a
  no-change west reconfigure; extending this there needs that check to see a
  compiler change).

**Acceptance:** gate `toolchain-change-reconfigure` drives the real module
through real configures with two stand-in compilers whose real paths differ.
A configure with A succeeds. A configure with B in the same dir is refused and
writes a marker naming both. After the fresh step, a configure with B succeeds,
and the tree's compiler is B. Negative control: without the cmake change the
gate fails ("the refusal wrote no … .nros-toolchain-changed"). The `nros build`
half is unit-tested: the marker clears the configure state and only that, and
no marker means no action.

### W7 — Measure nucleo-f767zi

**Code area:** none yet. This item is a measurement.

- Not measured on 2026-11-07: the shared Zephyr 3.7 west workspace has no
  `hal_stm32` (`st/f7/stm32f767zitx-pinctrl.dtsi: No such file or
  directory`), and the host has no Renode.
- Work: measure the three cells in a west workspace that `nros setup
  nucleo-f767zi-zephyr` provisions, under Renode. File each failure it finds
  as an issue and add it to this phase.

**Acceptance:** all three cells measured, with each red cell filed.

### W8 — The book: three tabs on every starter, each probed

**Code area:** `book/src/starters/`, `book/src/tutorial/`, the probe rows in
`packages/boards/board-support.toml`.

- Today only `tutorial/run-a-talker.md` has all three tabs:
  - Rust only: `mps2-an385-bare-metal`, `mps2-an385-zephyr`,
    `threadx-riscv64`;
  - C++ only: `linux-host`;
  - C only: `freertos-posix`, `nucleo-f767zi`;
  - C and Rust: `mps2-an385-freertos`, `nuttx-arm`, `nuttx-riscv`,
    `threadx-linux`, `zephyr-native-sim`.
- Work:
  - Add the missing tabs to every starter and every Tutorial chapter.
  - Give each tab a probe track with its `lang`.
  - A track whose cell is red carries an `awaits` that names this phase's work
    item or the owning fix, and the tab stays.
  - Remove `book/src/starters/freertos-posix.md:18`, which says that the
    board takes only C and C++.
  - Re-measure the cells owned outside this phase (2342, 531, 3268) as they
    land, and drop their awaits.
- Depends on W1 to W4 and W7 for its awaits to clear, not to start.

**Acceptance:**
- every starter and Tutorial chapter has Rust, C and C++ tabs;
- every tab has a probe track;
- `check-book-audience` passes with no new baseline row.

## Acceptance (phase)

- Every cell of the matrix above is green and re-measured on main.
- Every user-book starter and Tutorial chapter shows the Rust, C and C++ tabs,
  each replayed by its own probe track, with no `awaits` left.
