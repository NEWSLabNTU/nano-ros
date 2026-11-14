# Bare-metal C++ action server (MPS2-AN385, no RTOS)

Serves the `example_interfaces/Fibonacci` action on `/fibonacci`, publishing feedback for each number, from an ordinary nano-ros **C++** component, on a Cortex-M3 with **no
RTOS at all**.

## Layout

A one-package workspace, the same shape as an RTOS board's C++ example:

| Path | What it is |
| --- | --- |
| `src/baremetal_cpp_action_server/` | the component: `CMakeLists.txt` (`nano_ros_add_node(...)`), `src/FibServer.cpp`, `include/baremetal_cpp_action_server/FibServer.hpp`, `package.xml` |
| `src/baremetal_cpp_action_server_bringup/` | what it runs on: `system.toml` (board, RMW, locator), `board-net.toml` (the image's network), `system.contract.yaml` (what the node declares, for sizing) |

`FibServer.cpp` has no `main` and names no board. The entry the build generates for the bringup's image constructs the class and calls `configure(node)` on the image's executor.

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
cd examples/mps2-an385-baremetal/cpp/action-server
nros build                    # prints the artifact and the QEMU command line
```

The image dials the router at the `locator` in `system.toml`
(`tcp/10.0.2.2:10420`: 10.0.2.2 is the host as seen from QEMU's slirp network).

The fixture lane builds it from its `[[workspace_fixture]]` row
(`builder = "cmake"`): `scripts/build/workspace-fixtures-build.sh baremetal cpp`.
`tests/emulator.rs`'s `test_qemu_baremetal_capi_e2e` pairs it with
`../action-client` against a router on that port; the client prints `Result received: [...]`.
