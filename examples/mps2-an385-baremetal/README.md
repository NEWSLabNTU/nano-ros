# examples/mps2-an385-baremetal — bare-metal Cortex-M3 on QEMU MPS2-AN385

No-RTOS Rust examples (cortex-m-rt / RTIC) with smoltcp networking, run under
`qemu-system-arm -machine mps2-an385`. Just module: **`qemu`**
(`just/qemu-baremetal.just`).

## Prerequisites

```sh
source ./activate.sh
just setup qemu               # nros setup mps2-an385-baremetal --rmw zenoh
                              # + micro-cdr/micro-xrce sources + zenoh-pico + QEMU
source setup.bash             # newlib arm-none-eabi-gcc 13.2 on PATH
```

The nros-provisioned toolchain is required — a distro `gcc-arm-none-eabi`
without newlib headers will not link.

## RMW selection

Board-driven, no knob: the board crate (`nros-board-mps2-an385`) selects
zenoh. XRCE over the UART is an image of the Rust workspace's pubsub bringup,
`baremetal_talker_xrce_serial` (`examples/workspaces/rust/src/pubsub_bringup`).

## Build & run one example

```sh
just qemu build               # all bare-metal examples (auto-discovered)
just qemu zenohd &            # router on tcp port 7450
just qemu talker              # boot rust/talker in QEMU
just qemu listener            # peer in a second shell
```

Test lanes: `just qemu test`, `test-basic`, `test-zenoh`, `test-all`.

## Cases

| Lang | Role | variants |
| --- | --- | --- |
| rust | talker / listener | plain |
| c | talker / listener | plain (issue 1512) |
| c | service-server / service-client | plain (issue 1512) |
| c | action-server / action-client | plain (issue 1512) |
| cpp | talker | plain (issue 1512) |

**The talkers are ordinary C/C++ components; the other C/C++ roles are not
yet.** The two talkers (`c/talker`, `cpp/talker`) are one-package CMake
workspaces, the same shape as an RTOS board's: the package at `src/<pkg>/` is a
component with no `main` and no board name, and `src/<pkg>_bringup/system.toml`
names the board. `nros build` generates the image's entry and links it on the
CMake road, where the board's C start (`nros_board_mps2_an385_start`, in
`nros-board-mps2-an385`'s `c/c_main.c`) is `main`: it brings up the clock, the
LAN9118 and the network, then runs the entry (phase-553 W4/W5). Their
`[[workspace_fixture]]` rows say `builder = "cmake"`
(`scripts/build/workspace-fixtures-build.sh baremetal c` / `cpp`).

Every other C/C++ role is still rooted in cargo, the shape this board needed
before it had a C start: a one-package workspace (RFC-0122 D3, phase-535 W6)
whose package at `src/<pkg>/` is a cargo crate, with `src/main.rs` booting the
board and `build.rs` compiling the role's C or C++ into it. The bringup names
that package as the image's OWNED entry (`entry = "<pkg>"`, RFC-0071 D19), so
`nros build` builds it on the cargo road; those rows say `builder = "cargo"`.
phase-553 W6 moves them to the talkers' shape.

`test_qemu_baremetal_capi_e2e` in `tests/emulator.rs` runs every C/C++ pair end
to end under QEMU, each against its own router.

The UART configurations (zenoh `serial/UART_0` pair, XRCE `custom://uart`
talker) are images of `examples/workspaces/rust/src/pubsub_bringup`:
`baremetal_{talker,listener}_serial` and `baremetal_talker_xrce_serial`
(phase-535 W6).

RTIC is the same board with another entry framework (board word
`rtic-mps2-an385`). Its talker/listener, service and action pairs are the
`baremetal_<role>_rtic` images of the Rust workspace's pubsub, service and
action bringups (phase-535 W6); `tests/emulator.rs` runs them.

`rust/dds/` is build support, not a case. Test-only e2e fixtures
(`rtic-run-plan-e2e`, `qemu-baremetal-main-e2e`) live under
`packages/testing/nros-tests/bins/`, not here (RFC-0026).

## Gotchas

- QEMU needs `-icount shift=auto` for clock/network sync — the run recipes and
  `nros_tests::qemu` helpers already pass it (`book-internals/src/testing/qemu-icount.md`).
