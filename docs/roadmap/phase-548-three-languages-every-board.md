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
| freertos-posix (zenoh) | ok (W1) | ok | 2342 |
| freertos-posix (cyclonedds) | **W1** | ok | ok |
| mps2-an385 bare metal (zenoh), Tutorial 1 to 10 | ok | 531 | 531 |
| mps2-an385 FreeRTOS (zenoh) | ok | ok | ok |
| mps2-an385 Zephyr (zenoh) | 3268 | ok | **W2** |
| Zephyr native_sim (zenoh) | 3268 | ok | **W2** |
| NuttX qemu-armv7a (zenoh) | ok | ok | **W3** |
| NuttX rv-virt (zenoh) | ok | ok | ok |
| ThreadX Linux (zenoh) | ok | ok | ok |
| ThreadX qemu riscv64 (zenoh) | ok | ok | **W4** |
| nucleo-f767zi Zephyr (zenoh), Renode twin, serial | 2358 resolved 2026-11-09; W8 re-measures | ok | ok |

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

**Cyclone half done (2026-11-07).** The new crate `nros-board-freertos-posix`
compiles the GCC_POSIX kernel with heap_3, the FreeRTOS platform port without
`net.c`, and the family task glue, mirroring
`cmake/board/nano-ros-board-freertos-posix.cmake`. `nros-board-freertos`
gained a `posix-port` feature: no lwIP bring-up, drain task or netif check,
no `.init_array` walk, and a heap_3 heap reporter. Cyclone links through the
board's own `rmw-cyclonedds` feature, as on linux, and
`nros-rmw-cyclonedds-sys`'s new `foreign-wake-bridge` feature carries the
cmake road's `NO_FOREIGN_WAKE` + `FOREIGN_WAKE_BRIDGE` (issue 1237). The
descriptor is `entry_kind = "board-run"` with a C-ABI `main`, like
threadx-linux.

The board crate is now picked only by the board's OWN names
(`builder::entry::deploy_candidates`). The image id and the platform word no
longer answer for a board they do not name. That fall-through also gave
`mps3-an536-freertos` and `s32z270-freertos` the mps2 crate.

Measured:
- `nros new fpr --board freertos-posix --template talker --lang rust --rmw
  cyclonedds`, `nros build`, then `nros run fpr_node --expect 'Publishing:'`:
  matched after 1.1 s.
- `nros new m3 --board mps3-an536-freertos --lang rust` now depends on
  `nros-board-mps3-an536-freertos`, and is refused naming the board:
  `nros::main!: unknown board 'mps3-an536-freertos'`.

- `nros new fz --board freertos-posix --template talker --lang rust --rmw
  zenoh`, `nros build`, then `nros run fz_node --router none --expect
  'Publishing:'` against a router on `tcp/0.0.0.0:7447`: matched after 1.1 s.

The zenoh cell needed one more fix after #3284 (issue 2342) landed. The cargo
road's router default was the board's gateway, so the image dialled the
FreeRTOS family's `tcp/192.0.3.1:7447` from a host process and failed with
`Executor::open failed: Transport(ConnectionFailed)`. The cmake road already
used loopback for a host-stack board (`NROS_LOCATOR_ENTRY_HOST_STACK`).
`board_net::default_locator` (the one rule both roads read since the board-derived router default landed) now gives the same answer: loopback for
`[board.net] stack = "host"`, the gateway otherwise. It changes only the
locator. The address constants stay, because threadx-linux hands them to NetX.

`nros run` starts no router for this image, so the run needs one already up.

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

**Status (2026-11-07): DONE.** Cause confirmed: `CONFIG_NROS_CPP_API` selects
`CONFIG_CPP`, and Zephyr 3.7's default standard is `CONFIG_STD_CPP11`, so the
generated entry TU compiled at `-std=c++11`. The generated application's
`prj.conf` (`builder/west_app.rs`, `render_prj_conf`) now adds `CONFIG_CPP=y`
and `CONFIG_STD_CPP17=y` when the image's `LANG` is `cpp`. The standard is
C++17, not 14: it is the `cxx_std_17` floor nros-cpp declares, and what every
hand-written Zephyr C++ leaf and the `nros new` single-package scaffold already
state. The file is merged first, so a board or image fragment that names a
newer standard still wins. A C image's `prj.conf` is byte-identical to before,
and a Rust one gets no C++ line. Unit test:
`the_prj_conf_states_the_cpp_standard_only_for_a_cpp_image`, which reads the
floor from `packages/api/nros-cpp/CMakeLists.txt`.
Measured in the box: `nros new --lang cpp --rmw zenoh`, `nros build`,
`nros run <ws>_node --expect 'Publishing:'` on `mps2-an385-zephyr` (router
started by hand on `0.0.0.0:7447`, as the starter page says) and on `zephyr`
(native_sim); both match, and `ros2 topic echo /chatter` receives from both.
The hand-written leaves, templates and test bins were swept: every Zephyr C++
application already states `CONFIG_STD_CPP17`; the ones without a standard
are C.

- Found while measuring W2, outside it: issue 2353 (`nros run` on
  mps2-an385 Zephyr finds no west workspace without `ZEPHYR_BASE` and starts
  no router for the board's `10.0.2.2` locator; resolved 2026-11-09 with
  issue 2301) and issue 2354 (a generated
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

**Done (2026-11-07).** Two separate things looked like one:

- With no router reachable, `rclcpp::init_in` returns `TransportError`
  (-100), and `c_app_main` returns at once. The console said nothing about
  why. It now prints `[app_thread] c_app_main returned, exit code 156` (the
  status as an exit code, `-100 & 0xff`), and with W4b the run ends with it.
  With a router on 7447 the image does not return.
- With a router, the talker printed `Hello World: 1` and then went silent:
  issue 2340, the same bug. The riscv64 port's `TX_DISABLE` asm was not
  `volatile`, so GCC hoisted it out of a zero-trip loop in
  `_tx_mutex_delete`, and that function returned with interrupts masked.
  newlib deletes a mutex inside `printf`, so after the first print the tick
  stopped. The fix is in
  `packages/boards/nros-board-threadx-port-riscv64/port/inc/tx_port.h`.
  Measured: the scaffolded C++ talker prints `Hello World: 1` to `10` in
  10.1 s.
- A third bug turned up while proving the lane: issue 2357. The C service
  pair on this board went red after an edit to `threadx_hooks.c`, because
  the server and the client opened their sessions with one zenoh ZID, and
  the router refused the second. The board's RNG seed came from the
  configured address, which every image of a pair shares. It now also folds
  in the virtio device MAC and the CLINT counter. Measured: the cell passes
  3 of 3 runs, 1.8 s each.

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

**Done (2026-11-07).** `trap.c` ends QEMU through the `virt` machine's SiFive
test finisher (0x100000): a trap ends it with status 134, after the register
dump and the line `[EXCEPTION] ending the run (exit status 134)`. The board's
strong `nros_board_app_returned` ends it with the app's own exit code (0 for
success, as on threadx-linux), after
`[app_thread] c_app_main returned, exit code N`. Measured with `nros run …
--timeout 120`:
- a deliberate store fault in the talker's second tick ends the run after
  2.1 s: `the target exited with code 1`;
- `c_app_main` returning with no router ends it after 0.1 s.

Found on the way: issue 2355 (`nros-c`'s `critical_section` impl drops the
restore token on non-arm targets).

QEMU itself exits with the code (156 for the no-router return). `nros run`
reports 1, because a runner's exit is the 0/1/2 contract
(`nros_extension::exit`). The status is on the console in both cases.

### W5 — `nros setup` outside a checkout reads the current index — DONE

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

**Done (2026-11-07).** `resolve_index` gained a rung, `own_sdk_root_index()`: the release's `share/nano-ros` index, or for an in-tree `nros` its own checkout's. It answers after the checkout walk-up and before the store's fetched cache, and `$NROS_INDEX_URL` still outranks it. The cache rung that remains prints a note when the cache is older than the binary, naming `NROS_INDEX_REFRESH=1`. `cmd::setup::tests` passes 54/54, including the acceptance test above.

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

**Status (2026-11-07): MEASURED.** C ok, C++ ok, Rust red (issues 2358 and
2359).

**Code area:** none yet. This item is a measurement.

- Not measured on 2026-11-07 at first: the shared Zephyr 3.7 west workspace
  has no `hal_stm32` (`st/f7/stm32f767zitx-pinctrl.dtsi: No such file or
  directory`), and the host had no Renode.
- Work: measure the three cells in a west workspace that `nros setup
  nucleo-f767zi-zephyr` provisions, under Renode. File each failure it finds
  as an issue and add it to this phase.

**Acceptance:** all three cells measured, with each red cell filed.

Measured in the `ros2` box (Humble, Zephyr 3.7, Renode 1.17.0 from the
store), on the serial link only (ethernet in Renode needs `CAP_NET_ADMIN`).
The west workspace is the one `book/src/starters/nucleo-f767zi.md` writes
(`hal_stm32` in its allowlist), given the 3.7 patch set by
`nros setup board nucleo-f767zi-zephyr --zephyr-workspace "$PWD"`. Each cell
is `nros new <ws> --board nucleo-f767zi-zephyr --template talker --lang <l>
--rmw zenoh`, then `nros run <ws>_node --expect 'Publishing:'`, then a
second `nros run <ws>_node` with `ros2 topic echo /chatter
std_msgs/msg/String --qos-reliability best_effort` beside it.

- **C: ok.** `nros run: matched after 8.7 s: Publishing: 'Hello World: 1'`;
  the echo printed `data: 'Hello World: 28'` onward.
- **C++: ok.** `nros run: matched after 8.3 s: Publishing: 'Hello World: 1'`;
  the echo printed `data: 'Hello World: 28'` onward.
- **Rust: red.** First, the starter's workspace has no `zephyr-lang-rust`,
  and the hint `nros setup board` prints names a fragment that does not exist
  by an absolute path west refuses (issue 2358). Second, the board stated
  `rust_target = "thumbv7em-none-eabihf"` while its image is soft-float
  (`CONFIG_FPU` off), so issue 2204's check refused the configure; fixed
  here, in the descriptor, to `thumbv7em-none-eabi`. Third, with both out of
  the way the image boots to `nros: zephyr workspace entry up (1 nodes)`
  and its timer callback never runs (issue 2359). The same template
  publishes on `mps2-an385-zephyr`, so the fault is the Rust entry on a
  serial session, not Rust on Zephyr.
- The starter's `echo 'locator = ...' >> system.toml` step wrote a second
  `locator` under `[image.zephyr]`: the scaffold already writes it (issue
  2290), and `nros build` stopped with ``duplicate key `locator` ``. Fixed
  here: the step now shows the scaffolded line.
- Issue 2353 did not show here: the `nros new` workspace sits inside the
  west workspace, and `nros run` found it with `ZEPHYR_BASE` unset.
- `nros setup nucleo-f767zi-zephyr` exits 1 in the box: the Zephyr SDK's
  host-tools step needs `file`, which the box lacks
  (`sudo apt-get install file`). The build does not use the host tools, so
  no cell flips on it.

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

### W9 — The RISC-V toolchain file finds rust-lld on the pinned toolchain — DONE

**Code area:** `cmake/toolchain/riscv64-threadx.cmake`,
`packages/api/nros-c/cmake/nros-rtos-helpers.cmake` (`nros_host_rustlib_bin`),
`nros-cli-core` `cmd/setup.rs`.

- Measured: a project outside this checkout, on a host whose rustup default
  is a bare `nightly` (no `llvm-tools`), failed at configure with
  `llvm-ar not found alongside rust-lld`. `nros_host_rustlib_bin` asked
  `rustc --print sysroot` before anything had pinned the toolchain, so it got
  the default's sysroot, while every cargo command of the same build runs the
  `rust-toolchain.toml` pin (issue 1914). `nros setup` added `llvm-tools` and
  the board targets to rustup's default only, so on that host it provisioned
  a toolchain that no CMake build used.
- Reproduced without global state: a private `RUSTUP_HOME` whose toolchains
  are the host's and whose default is `nightly`.

**Done (2026-11-07).**
- `nros_host_rustlib_bin` calls `nros_pin_default_rust_toolchain()` first,
  and passes an explicit `-DRust_TOOLCHAIN` to the query.
- Both refusals in the toolchain file name the toolchain they searched, and
  give two remedies: `nros setup rv-virt-threadx`, or
  `rustup component add llvm-tools --toolchain <tc>`.
- `nros setup` installs each default-toolchain need on the pin as well. The
  default's row stays, because the cargo road still runs rustup's default.
- Measured on the repro: the default `nightly` configures, now on 1.99.0, and
  so does `-DRust_TOOLCHAIN=stable`. `-DRust_TOOLCHAIN=nightly` refuses with
  the named remedy.

## Acceptance (phase)

- Every cell of the matrix above is green and re-measured on main.
- Every user-book starter and Tutorial chapter shows the Rust, C and C++ tabs,
  each replayed by its own probe track, with no `awaits` left.

## Issues homed here (2026-11-07)

| issue | why it belongs here |
| --- | --- |
| [#2313](../issues/2313-zephyr-generated-rust-entry-cannot-run-cyclone.md) | A generated Rust Zephyr entry cannot run Cyclone DDS: the Rust cell on every Zephyr board with Cyclone, beside issue 2354. |
| [#2340](../issues/archived/2340-threadx-riscv64-generated-cpp-talker-hangs-after-first-publish.md) | RESOLVED 2026-11-07 by W4. The generated C++ talker on threadx-riscv64 publishes once then spins: the C++ cell of W4 on that board. |
| [#2301](../issues/archived/2301-zephyr-run-plan-cannot-see-kconfig-locator.md) | RESOLVED 2026-11-09 with issue 2353. `nros run` on a Zephyr board sees no default locator, so `--router auto` starts no router. This is the Zephyr arm that 9c52ae9e34's one rule leaves to Kconfig, and the same symptom as issue 2353's second half. |
| [#2367](../issues/2367-native-sim-run-link-ignores-config.md) | `nros run` on native_sim takes its link from the host default, not from the locator its build baked. Found fixing issue 2301. |
| [#2368](../issues/2368-why-names-no-config-file.md) | `--why` reports a locator read from the build's `.config` without naming the file. Found fixing issue 2301. |
| [#2355](../issues/archived/2355-nros-c-critical-section-drops-restore-token.md) | RESOLVED 2026-11-09: release restores the token its acquire saved. `nros-c`'s `critical_section` release passes a literal 0, which means `TX_INT_DISABLE` on ThreadX riscv64. Found while tracing issue 2340 on W4's board. |
| [#2360](../issues/2360-zephyr-rust-registration-stack-frames.md) | A Rust Zephyr entry's registration frames may overflow 32 KiB of main stack for an action image. This is the Rust cell on Zephyr, found fixing issue 2359. |
| [#2365](../issues/2365-nucleo-f767zi-c-talker-ram-overflow.md) | Issue 2359's 16 -> 32 KiB main-stack raise also grew the two zenoh task stacks, which inherit main's size, so the nucleo-f767zi C talker overflowed RAM by 7,872 B and tier 2 stopped in the fixture build. The raising boards now state the task stack; a gate keeps it that way. |
