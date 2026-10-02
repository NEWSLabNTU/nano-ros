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
zenoh. XRCE has its own dedicated example (`talker-xrce`) that swaps the board
transport feature instead.

## Build & run one example

```sh
just qemu build               # all bare-metal examples (auto-discovered)
just qemu zenohd &            # router on tcp port 7450
just qemu talker              # boot rust/talker in QEMU
just qemu listener            # peer in a second shell
```

RTIC service/action runners: `just qemu rtic-service-server`,
`rtic-service-client`, `rtic-action-server`, `rtic-action-client`.
Test lanes: `just qemu test`, `test-basic`, `test-zenoh`, `test-all`.

## Cases

| Lang | Role | variants |
| --- | --- | --- |
| rust | talker / listener | plain, `-rtic`, `-rtic-mixed`, `serial-*` (UART transport) |
| rust | service-server / service-client | `-rtic` |
| rust | action-server / action-client | `-rtic` |
| rust | talker-xrce | XRCE-DDS transport variant |
| c | talker / listener | plain (issue 1512) |
| c | service-server / service-client | plain (issue 1512) |
| c | action-server / action-client | plain (issue 1512) |
| cpp | talker | plain (issue 1512) |

**The C/C++ leaves' SHAPE differs from every other C/C++ example in the tree,
and the reason is the board rather than the API** — read
[`c/talker/README.md`](c/talker/README.md) before copying one. On an RTOS a C
example is a C program: the RTOS supplies `main`, a C startup and a C platform
port, and `libnros_c.a` is linked *in*. This board has no RTOS, so the reset
vector (`cortex-m-rt`), the clock (CMSDK Timer0) and the network (LAN9118 +
smoltcp) are all Rust with no C entry point. The leaf is therefore rooted in
cargo: `src/main.rs` boots the board and calls `app_main()`, and `build.rs`
compiles the leaf's `src/*.c` (or `src/talker.cpp`) into the image.
`nano_ros_add_executable` is not available on this platform. Each leaf is a
`[[fixture]]` row with `builder = "cargo"`, built by `just qemu build-fixtures`
(`scripts/build/fixtures-build.sh baremetal c` / `cpp`).

Status: build-only. It boots in QEMU and reaches session open, but it dials
`NROS_ENTRY_LOCATOR`'s empty bottom rung rather than the `[image.*] locator` in
its `system.toml`, so there is no runtime lane yet — see issue 1512.

`rust/dds/` is build support, not a case. Test-only e2e fixtures
(`rtic-run-plan-e2e`, `qemu-baremetal-main-e2e`) live under
`packages/testing/nros-tests/bins/`, not here (RFC-0026).

## Gotchas

- QEMU needs `-icount shift=auto` for clock/network sync — the run recipes and
  `nros_tests::qemu` helpers already pass it (`docs/reference/qemu-icount.md`).
