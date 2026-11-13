# Phase 553 — One C/C++ build method: a ROS 2 CMake package on every board

**Status (2026-11-13). IN PROGRESS: W1 to W4 done; W5 to W7 open.**
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
of `.bss`, upward toward the stack. The main stack bounds that layout too:
W7 gives it a stated size and a guard, and caps the `_sbrk` heap below the
guard.

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

- **Status:** DONE (2026-11-12, #3645). Builds on W1 (`c-libc`).

cortex-m-rt runs no `.init_array`, and its `link.x` does not place
`.preinit_array`/`.init_array` at all (an orphan section with no bounds).
newlib's `__libc_init_array` is not usable either: it also calls `_init` from
`crti.o`, which a `-nostartfiles` link does not have (measured: `undefined
reference to '_init'`).

- `packages/boards/nros-board-mps2-an385/c-runtime.x`, copied next to
  `memory.x` by the board's build script, is a C-rooted image's link script:
  it includes `link.x`, then keeps and bounds both arrays in flash. An
  `INSERT` in a second `-T` script, or in a plain input script, finds no
  `.rodata` (measured); cortex-m-rt honours `INSERT`s only from a script read
  inside its own, hence the include.
- `nros_board_mps2_an385_start(app)` (board crate, `c-libc` only) is the
  C-ABI start: it walks the preinit and init arrays, then brings the board up
  (`run_bare(Config::from_image(), …)`) and runs `app`. W4 builds the road
  on it.

Measured on the RFC-0130 D4 probe linked with `-Tc-runtime.x`: a
namespace-scope C++ object whose constructor sets a flag to 42 reads 42 when
the application starts (`ctor flag: 42`), and the C talker then publishes and
logs as in W1.

### W3 — No `-lgcc` on the C-rooted link

- **Status:** DONE (2026-11-13, #3716, with W4, whose road generates the link line it constrains)

Measured in RFC-0130 D4: libgcc drags ARM unwind code needing
`__exidx_start`/`__exidx_end` and `abort`. The C-rooted link line omits it,
and a C++ component builds with `-fno-exceptions -fno-rtti` (the W2 probe's
C++ object did).

Acceptance: the generated link line for a C and a C++ image carries no
`-lgcc` and links with no undefined symbol; a check fails if it reappears.

Done. The board overlay (`cmake/board/nano-ros-board-mps2-an385-baremetal.cmake`)
links `-nostdlib` and names newlib-nano and rdimon only; the staticlib's
compiler-builtins supplies the helpers. Both talkers' `link.txt` carry no
`-lgcc` and link clean. `just check baremetal-platform-arms` rule F fails on a
bare-metal overlay that drops `-nostdlib` or names libgcc in any spelling
(mutation-tested both ways).

### W4 — The CMake-rooted road on mps2-an385

- **Status:** DONE (2026-11-13, #3716)

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

What landed:
- **Family.** `BoardFamily::Baremetal` in `nros-entry-lower`, runner
  `nros_board_rtos_run_components_in`, boot shape `App`, C++ class
  `rclcpp::board::BaremetalBoard` (ThreadX's lifecycle; nothing in it names a
  kernel). Its sched-task key is the empty one, so a Rust image on these keys
  resolves exactly what it did before. Every name of the bare-metal descriptor
  is a `BOARD_KEYS` row, which lifts `nros new`'s refusal; the guard stays for
  a board key the table does not know.
- **Staticlib.** `nros_image_crate(... BAREMETAL_BOARD <board>)` adds the board
  crate from `cmake/NanoRosBareMetalPlatform.cmake`'s board-crate row:
  `c-libc`, the link feature the image's transport implies (`serial` when the
  image env's `NROS_LINK_IP` is `0`, else `ethernet`), `rmw-<rmw>` when the
  crate declares it (that installs the link's RX interrupt as the session's
  wake; without it the session states no readiness form and is refused), and
  an anchor on the C-ABI start.
- **Link scripts.** The cargo placer copies the top-level `*.x` of
  cortex-m-rt's, the board's and the PAC's `$OUT_DIR` into one directory the
  staticlib target hands the link with `-L`; the overlay adds `-Tc-runtime.x`.
- **Main.** The board's `c/c_main.c` is the C-rooted image's `main`: it hands
  the entry's `app_main` to `nros_board_mps2_an385_start` and carries the
  runner's exit code back through `nros_app_main_returned`.
- **Executor storage** is the entry's `NROS_CPP_EXECUTOR_STORAGE_SIZE`, sized
  by the build: 5,880 B for the talker (the D4 probe hand-sized 96 KiB).
- Four defects met on the way, each fixed for its class: a locator with `#`
  (every serial one) was dropped from the compiler command line by CMake on
  the non-Zephyr roads too, so `nros_locator_c_define` now spells it for all
  four sites; the bare-metal platform module lacked the in-tree
  `_NANO_ROS_PREFIX` (a C++ message glue crate named `nros-serdes` three
  directories above the checkout); the board's capability defines reached the
  app but not a component library (ThreadX's phase-246 fix, applied here); and
  the module did not include `NanoRosLink.cmake`, so no
  `nros_app_register_backends` was generated.

Measured 2026-11-13, scaffolded workspaces (`nros new <n> --lang c|cpp
--platform baremetal`, first image on the UART at 115200):

| | C talker | C++ talker |
|---|---|---|
| `nros build` for `mps2-an385-baremetal` | builds | builds |
| `ros2 topic echo /chatter` (rmw_zenoh_cpp, router from `nros run`) | `Hello World: 20` | `Hello World: 24` |
| the same package, image board `mps2-an385-freertos` | builds | builds |
| `-lgcc` on the link line | none | none |
| text / data / bss (B) | 542,252 / 2,612 / 173,220 | 531,876 / 2,604 / 172,296 |
| `mem-report` RAM | 175,840 B | 174,900 B |
| stack high-water | 11,068 B | 11,004 B |

The RAM is dominated by the Rust allocator's arena (132,816 B,
`nros_platform_mps2_an385::memory::HEAP`); newlib's heap grows from `end`
(`__sheap`) above `.bss`, the layout W1 chose. The stack figure is a runtime
high-water mark, not a static floor: QEMU zero-fills RAM, so after ~20
publishes over the UART the deepest non-zero byte below `_stack_start`
(0x20400000) is how deep the stack went. cortex-m-rt's stack is the RAM above
`.bss` (about 4 MiB here), so the margin is large; the static floor walk
(`NanoRosStackFloor.cmake`) checks a FreeRTOS carrier's stated stack only and
has no bare-metal arm yet. `just check dep-chain`: 7 of 7 cells pass.

### W5 — Migrate the bare-metal C/C++ talkers

- **Status:** OPEN (W4 is done)

`examples/mps2-an385-baremetal/{c,cpp}/talker` become one-package workspaces
whose package is an ordinary component, booted by the generated entry.

Acceptance: their fixture rows are workspace rows on the cmake road, and
their runtime cells pass.

### W6 — Retire the cargo-built C/C++ bare-metal workspaces

- **Status:** OPEN. Owner: to be assigned; this is not claimed with
W1 to W5 (the session that migrated these workspaces is the expected owner).

The ten `examples/mps2-an385-baremetal/{c,cpp}/{listener,service-*,action-*}`
workspaces whose package is a hand-written cargo crate move to ordinary
component packages, and `nros-cc-flags`' leaf-only modules lose their last
consumers (measured before deleting).

Acceptance: no tracked `Cargo.toml` + `build.rs` compiles C or C++ for an
example; `test_qemu_baremetal_capi_e2e` passes against the new rows.

### W7 — A bounded, guarded stack on the C-rooted image

- **Status:** DONE (2026-11-13). FreeRTOS task stacks are not covered: [issue 2507](../issues/2507-freertos-task-stack-overflow-reads-as-malloc-failed.md).

Issue 2494. On bare-metal mps2-an385 the main stack is neither bounded nor
guarded: cortex-m-rt grows it down from `_stack_start` (0x2040_0000), the
`_stack_size` / `_heap_*` symbols in `mps2-an385.x` are read by nothing, and
the Cortex-M3 has no `MSPLIM`. On the C-rooted road W1 makes newlib's `_sbrk`
grow UP from `end` (`__sheap`) toward that same stack, so a collision corrupts
memory with no diagnostic. W4 measured the talkers' stack high-water at about
11 KiB with almost 4 MiB free between the two, so nothing collides today;
nothing would say so if it did.

- A real stack region in `c-runtime.x`: `_stack_size` bounds it, and link
  `ASSERT`s fail the link when `.bss` plus the newlib heap reservation would
  reach it.
- newlib's `_sbrk` on this image refuses to grow past the stack's lower limit
  (returns `-1`/`ENOMEM`, so `malloc` fails loudly), or a canary at the limit
  that the board checks; the measurement decides which.
- The stack size is a knob the image states, so the stack-floor check (issue
  2400, its bare-metal arm) can compare the measured floor with it.

Acceptance:
- An image whose `.bss` plus stated stack exceeds RAM fails to LINK, naming
  the knob.
- A test image that allocates until `malloc` fails gets `NULL` before its
  heap reaches the stack region (negative control: the same image without the
  guard corrupts the stack, measured).
- The C and C++ talkers still run, with their stated stack above the measured
  floor.

Landed, for every mps2-an385 bare-metal image (cargo-rooted and C-rooted;
`c-runtime.x` includes `link.x`, which includes the board's `memory.x`):

- `memory.x` states the main stack: `_stack_size` from `nros-stack.x`, which
  the board's build script writes from `NROS_BAREMETAL_STACK_KB` (default
  256 KiB). The stack sits at the top of RAM, `_stack_end` is its real lower
  bound, and a 1 KiB guard (`_nros_stack_guard`) sits below it. `ASSERT`s fail
  the link when the statics reach the guard, or when the size is not whole
  KiB.
- The guard is an MPU region with no access, armed first thing at boot on
  both start paths (`stack_guard::install`). A canary was not chosen: it
  reports an overflow only after the stack has already written over what lay
  below. Flip-link was measured and rejected: QEMU's mps2-an385 accepts a
  write below the SRAM base without a fault (QEMU 6.2 and 11.0), so a stack
  placed at the RAM base overflows silently. The MPU faults on both QEMUs.
  MemManage stays off and `HFNMIENA` clear, so the fault escalates to
  HardFault and the frame lands in the guard. The board's HardFault handler
  prints `nros: stack overflow: …` (any other fault prints its `pc`/`lr`) and
  exits QEMU with a failure status. The guard catches a stack that grows past
  its limit; one frame larger than 1 KiB can still step over it.
- The C-rooted image's `_sbrk` is the board's (`c/c_main.c`, strong over
  rdimon's weak one): newlib's heap grows from `end` and stops at the guard
  with `ENOMEM`. rdimon's stops only at the current stack pointer.
- The board states the stack for `nros build`'s floor check
  (`nros-stack-stated.json`, root `main`, knob `NROS_BAREMETAL_STACK_KB`).
  The check does not yet arm for this platform; turning it on belongs to
  phase-557.

Measured (QEMU 11.0, `-icount shift=auto`):

| image | stack | result |
| --- | --- | --- |
| C talker (scaffolded, serial) | 256 KiB (default) | runs, 14 published in 20 s |
| C talker | 4 KiB | `nros: stack overflow`, QEMU exits 1, nothing published |
| C++ talker | 256 KiB | runs, 15 published in 20 s |
| Rust talker (`examples/mps2-an385-baremetal/rust/talker`, ethernet) | 256 KiB | runs, 25 published in 25 s |
| Rust talker | 4 KiB | `nros: stack overflow`, QEMU exits 1 |
| Rust talker | 4000 KiB | link fails: "the image's statics run into the main stack and its guard. Lower NROS_BAREMETAL_STACK_KB …" |

Heap acceptance, a C talker that mallocs 16 KiB blocks until `NULL`, then
frees them and runs on:

| board | `_sbrk` | heap top at `NULL` | where that is |
| --- | --- | --- | --- |
| with W7 | the board's | 0x203b_fa08, 229 blocks | below the guard (0x203b_fc00) |
| before W7 (negative control) | rdimon's (weak) | 0x203f_ba90, 244 blocks | 15.9 KiB below the live stack pointer, 246 KiB into what is now the stack region |

Both runs then published normally. The negative control shows the overlap
rather than a corruption: the heap reached within 15.9 KiB of the stack
pointer, and a stack deeper than that writes over live heap blocks with no
diagnostic.

Not covered: FreeRTOS images. Their task stacks are heap_4 blocks, and an
overflow already surfaces (as `MALLOC FAILED`, the misattribution recorded in
`freertos_config.rs`), so guarding them means reprogramming the MPU on every
context switch. That is separate work, tracked in
[issue 2507](../issues/2507-freertos-task-stack-overflow-reads-as-malloc-failed.md).

## Not in scope

- NuttX (RFC-0130 D7, decided): this phase changes no NuttX road; the guest
  make road's work is phase-550's.
- The cargo-entry design (a generated cargo entry compiling component sources
  by convention): superseded by RFC-0130 and never built.
