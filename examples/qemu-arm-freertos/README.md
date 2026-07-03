# examples/qemu-arm-freertos — FreeRTOS on QEMU MPS2-AN385 (Cortex-M3)

C, C++ and Rust examples on FreeRTOS + lwIP, run under
`qemu-system-arm -machine mps2-an385`. Just module: **`freertos`**
(`just/freertos.just`).

## Prerequisites

```sh
source ./activate.sh          # exports FREERTOS_DIR + FREERTOS_PORT (GCC/ARM_CM3)
just freertos setup           # = nros setup qemu-arm-freertos + host deps
```

## RMW selection

Board-driven: the examples carry no `rmw-*` feature knob — the backend comes
from the board crate (`nros-board-mps2-an385-freertos`, zenoh). zenoh is the
only supported backend on this platform (see the
[coverage matrix](../README.md)).

## Build & run one example

```sh
just freertos build-examples             # all C/C++/Rust examples
just freertos zenohd &                   # router on tcp port 7451
just freertos talker                     # build + boot rust/talker in QEMU
just freertos listener                   # peer in a second shell
```

Test lanes: `just freertos test`, `test-all`.

## Cases

| Role | c | cpp | rust |
| --- | --- | --- | --- |
| talker / listener | yes | yes | yes (+`_entry`) |
| service-server / service-client | yes | yes | yes (+`_entry`) |
| action-server / action-client | yes | yes | yes (+`_entry`) |

The `<case>_entry` Rust variants are Entry-pkg siblings (`nros::main!()` run-plan
shape); the underscore naming is an interim exception (RFC-0026, phase-275).
`rust/dds/` is a shared build-support crate, not an example case.

## Gotchas

- App task stack must stay large (64 KB) — the executor arena lives on the
  task stack; too small manifests as lwIP "Invalid mbox" (see
  `docs/reference/platform-implementation-notes.md`).
- Build skips with a clear message if `$FREERTOS_DIR/include` is missing —
  run `just freertos setup`.
