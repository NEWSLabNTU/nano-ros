# examples/qemu-riscv64-threadx — ThreadX/NetX Duo on QEMU (riscv64)

C, C++ and Rust examples on Eclipse ThreadX with NetX Duo networking, run
under `qemu-system-riscv64`. Just module: **`threadx_riscv64`**
(`just/threadx-riscv64.just`).

## Prerequisites

```sh
source ./activate.sh          # exports THREADX_DIR / NETX_DIR
just threadx_riscv64 setup    # = nros setup qemu-riscv64-threadx + host deps
```

## RMW selection

Rust: Cargo features `rmw-zenoh` (default) / `rmw-cyclonedds` on each example.
C/C++: built through the fixtures manifest (`-DNROS_RMW`). zenoh is the
supported backend across all six roles; cyclonedds is experimental
talker/listener only (see the [coverage matrix](../README.md)).

## Build & run one example

```sh
just threadx_riscv64 build-examples      # all rust examples via the fixtures lane
just threadx_riscv64 zenohd &            # router
just threadx_riscv64 talker              # boot rust/talker in QEMU
just threadx_riscv64 listener            # peer in a second shell
```

Test lanes: `just threadx_riscv64 test`, `test-all`.

## Cases

| Role | c | cpp | rust |
| --- | --- | --- | --- |
| talker / listener | yes | yes | yes |
| service-server / service-client | yes | yes | yes |
| action-server / action-client | yes | yes | yes |

`rust/dds/` is build support, not a case.

## Gotchas

- Build skips cleanly if ThreadX/NetX headers or `riscv64-unknown-elf-gcc`
  are missing — run setup first.
- NetX Duo BSD `SO_RCVTIMEO` takes `nx_bsd_timeval*`, not int ms
  (`docs/reference/platform-implementation-notes.md`).
- Known limitation: firmware NULL `c_app_main` after rebuild on some hosts —
  issue #127.
