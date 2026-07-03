# examples/qemu-arm-baremetal — bare-metal Cortex-M3 on QEMU MPS2-AN385

No-RTOS Rust examples (cortex-m-rt / RTIC) with smoltcp networking, run under
`qemu-system-arm -machine mps2-an385`. Just module: **`qemu`**
(`just/qemu-baremetal.just`).

## Prerequisites

```sh
source ./activate.sh
just qemu setup               # nros setup qemu-arm-baremetal --rmw zenoh
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

## Cases (rust only — no bare-metal C/C++ harness exists)

| Role | variants |
| --- | --- |
| talker / listener | plain, `-rtic`, `-rtic-mixed`, `serial-*` (UART transport) |
| service-server / service-client | `-rtic` |
| action-server / action-client | `-rtic` |
| talker-xrce | XRCE-DDS transport variant |

`rust/dds/` is build support, not a case. Test-only e2e fixtures
(`rtic-run-plan-e2e`, `qemu-baremetal-main-e2e`) live under
`packages/testing/nros-tests/bins/`, not here (RFC-0026).

## Gotchas

- QEMU needs `-icount shift=auto` for clock/network sync — the run recipes and
  `nros_tests::qemu` helpers already pass it (`docs/reference/qemu-icount.md`).
