# Bare-metal C talker (MPS2-AN385, no RTOS)

Publishes `std_msgs/String` `"Hello World: N"` on `/chatter` once a second from
an ordinary nano-ros **C** application, on a Cortex-M3 with **no RTOS at all**.

This is the first C leaf in either bare-metal family (issue 1512). Read the next
section before copying it: the *shape* differs from every other C example in the
tree, and the reason is the board, not the C API.

## Why the link root is Rust here

On FreeRTOS, NuttX, Zephyr and ThreadX a C example is a C program: the RTOS
supplies `main`, a C startup and a C platform port, and `libnros_c.a` is linked
*in*. `examples/mps2-an385-freertos/c/talker/` is that shape — a `CMakeLists.txt`
calling `nano_ros_add_executable`, nine lines long.

A board with no RTOS has none of those pieces in C:

| What an image needs | Where it lives on this board |
| --- | --- |
| reset vector, `.data` copy, `.bss` zero | `cortex-m-rt` (Rust) |
| monotonic clock | CMSDK Timer0, `nros_platform_mps2_an385::clock` (Rust) |
| network | LAN9118 + `smoltcp`, `nros_board_mps2_an385::init_hardware` (Rust) |
| console | semihosting, `cortex-m-semihosting` (Rust) |
| heap | `zpico_alloc::FreeListHeap` in `.bss` (Rust; 128 KB, `NROS_HEAP_SIZE`) |

Only the last one has a C-visible face (`malloc`/`free`, from
`nros-baremetal-common`'s `libc-heap`). There is no C entry point for the other
four, so a C-rooted link would have to re-implement them — a second bring-up
path beside the Rust one, which is the duplication this repo treats as a defect.

So the image is rooted in **cargo**, `src/main.rs` owns the reset entry and the
board bring-up, and `build.rs` compiles `src/talker.c` into it. That is exactly
the arrangement `<nros/app_main.h>` already documents for this platform:
`NROS_APP_MAIN_REGISTER()` emits `void app_main(void)`, and "per-platform startup
chains call this after platform init (network, executor arena, board hw)".

`cmake/platform/nano-ros-baremetal.cmake` and
`cmake/board/nano-ros-board-mps2-an385-baremetal.cmake` still exist and are what
an **out-of-tree** C parent build would use. They remain without a consumer, and
issue 1512 records the three things they would need first (a `SECTIONS` linker
script — the one the overlay names is a `MEMORY{}`-only `cortex-m-rt` fragment —
a C reset/vector startup, and a C-callable board init).

## What is ordinary about it

`src/talker.c` is the same program as its FreeRTOS sibling: `nros_support_init`,
`rclc_node_init_default`, `rclc_publisher_init_default`, a 1 Hz
`nros_timer_init`, `nros_executor_init`, `rclc_executor_spin_period`. The C API
is not subset here — `nros-c` is built with the `platform-mps2-an385` arm and
carries its whole surface.

## Two bare-metal C facts worth knowing

* **No libc.** No `stdio.h`, no `getenv`, no `signal`. `memset`/`memcpy`/`malloc`
  and the `str*` helpers come from `nros-baremetal-common`; anything else is an
  undefined symbol at link time, which is the honest answer.
* **`printf`-style logging does not substitute.** `nros-baremetal-common`'s
  `vsnprintf` copies its format string **verbatim** (a deliberate choice: an
  unsubstituted message still names the failure, where an empty buffer is
  silence). So `NROS_LOG_INFO(logger, "n=%d", n)` prints `n=%d` on this
  platform. Build the text and call `nros_log_emit_at`, as this example does.

## Build

```sh
source ./activate.sh          # puts this checkout's `nros` on PATH
nros sync examples/mps2-an385-baremetal/c/talker
cd examples/mps2-an385-baremetal/c/talker && cargo build --release
```

`nros sync` writes the `[patch.crates-io]` rows and the board's
`.cargo/config.toml` (triple, QEMU runner, `--gc-sections`,
`CC_thumbv7m_none_eabi`). `build.rs` then runs `nros generate c` for the
`<depend>` list in `package.xml` and compiles the bindings beside `talker.c`.

## Status

Build-only. `matrix::CELLS` carries this cell as `BuildOnly`, and issue 1512
records what a runtime lane needs next: this image dials
`NROS_ENTRY_LOCATOR`, whose bare-metal bottom rung is the empty string, so the
backend picks its own default instead of the `[image.*] locator` in
`system.toml`. Baking that is an entry-codegen job, not a thing this leaf should
re-derive — `<nros/entry_config.h>` is the one producer of that ladder
(issue 0946).
