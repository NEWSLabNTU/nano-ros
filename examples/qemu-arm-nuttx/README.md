# examples/qemu-arm-nuttx — NuttX on QEMU (Cortex-M3)

C, C++ and Rust examples on the NuttX RTOS, run under QEMU. Just module:
**`nuttx`** (`just/nuttx.just`).

## Prerequisites

```sh
source ./activate.sh          # exports NUTTX_DIR / NUTTX_APPS_DIR
just nuttx setup              # = nros setup qemu-arm-nuttx + host deps + apps staging
```

`nros setup` provisions the NuttX kernel/apps source submodules — no manual
`git submodule update` needed.

## RMW selection

Board-driven: examples carry no `rmw-*` feature knob (the platform selects the
backend); zenoh is the only supported backend here (see the
[coverage matrix](../README.md)).

## Build & run one example

```sh
just nuttx build-examples                # all arm C/C++/Rust examples
just nuttx zenohd &                      # router
just nuttx talker                        # build + boot rust/talker in QEMU
just nuttx listener                      # peer in a second shell
```

Test lanes: `just nuttx test`, `test-all`.

## Cases

| Role | c | cpp | rust |
| --- | --- | --- | --- |
| talker / listener | yes | yes | yes (+`_entry`) |
| service-server / service-client | yes | yes | yes (+`_entry`) |
| action-server / action-client | yes | yes | yes (+`_entry`) |

`<case>_entry` = Entry-pkg siblings (`nros::main!()`); underscore naming is an
interim exception (RFC-0026, phase-275). `rust/dds/` is build support, not a
case. The riscv NuttX lane lives at
[`examples/qemu-riscv-nuttx/`](../qemu-riscv-nuttx/), not here.

## Gotchas

- NuttX spin uses `sem_timedwait` (pthread condvar hangs) — see
  `docs/reference/platform-implementation-notes.md`.
