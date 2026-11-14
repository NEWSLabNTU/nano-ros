# Bare-metal C listener (MPS2-AN385, no RTOS)

Subscribes to `std_msgs/String` on `/chatter` and prints each sample as `I heard: [...]`, from an ordinary nano-ros **C** component, on a Cortex-M3 with **no
RTOS at all**.

## Layout

A one-package workspace, the same shape as an RTOS board's C example:

| Path | What it is |
| --- | --- |
| `src/baremetal_c_listener/` | the component: `CMakeLists.txt` (`nano_ros_add_node(... TYPED)`), `src/Listener.c`, `package.xml`, `nros-codegen.toml` |
| `src/baremetal_c_listener_bringup/` | what it runs on: `system.toml` (board, RMW, locator), `board-net.toml` (the image's network) |

`Listener.c` has no `main` and names no board. `RCLC_C_COMPONENT` emits the C-ABI factory and configure hooks; the entry the build generates for the bringup's image calls them on the image's executor.

## How it boots with no RTOS

On this board the entry's `main` is the board's C start,
`nros_board_mps2_an385_start` (`nros-board-mps2-an385`'s `c/c_main.c`). It runs
the board bring-up written in Rust (CMSDK Timer0 clock, LAN9118 + smoltcp
network, semihosting console, the `nros_log` sinks), then calls the entry. The
board crate, `cortex-m-rt` and the PAC come in through the image's Rust
staticlib, so the link is rooted in CMake like every RTOS board's (phase-553
W4). The C library is newlib-nano, so `printf` and `snprintf` work.

## Build and run

```sh
source ./activate.sh          # puts this checkout's `nros` on PATH
cd examples/mps2-an385-baremetal/c/listener
nros build                    # prints the artifact and the QEMU command line
```

The image dials the router at the `locator` in `system.toml`
(`tcp/10.0.2.2:10300`: 10.0.2.2 is the host as seen from QEMU's slirp network).

The fixture lane builds it from its `[[workspace_fixture]]` row
(`builder = "cmake"`): `scripts/build/workspace-fixtures-build.sh baremetal c`.
`tests/emulator.rs`'s `test_qemu_baremetal_capi_e2e` pairs it with
`../talker` against a router on that port; the listener hears the talker.
