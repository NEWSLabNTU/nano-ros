# examples/qemu-armv7a-nuttx — NuttX on QEMU (Cortex-M3)

C, C++ and Rust examples on the NuttX RTOS, run under QEMU. Just module:
**`nuttx`** (`just/nuttx.just`).

## Prerequisites

```sh
source ./activate.sh          # exports NUTTX_DIR / NUTTX_APPS_DIR
just setup nuttx              # = nros setup qemu-armv7a-nuttx + host deps + apps staging
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
```

The peers (listener, service and action roles) are `nuttx_<role>` images of
the behaviour bringups in `examples/workspaces/{rust,c,cpp}` (phase-535 W5,
RFC-0122 D1); `just nuttx build-examples` builds them too.

Test lanes: `just nuttx test`, `test-all`.

## Cases

| Role | c | cpp | rust |
| --- | --- | --- | --- |
| talker (the board's road witness) | yes | yes | yes |
| typed-carrier | yes | | |

Listener, service and action roles: `examples/workspaces/<lang>/src/{pubsub,service,action}_bringup`, images `nuttx_<role>`.

## Gotchas

- NuttX spin uses `sem_timedwait` (pthread condvar hangs) — see
  `book-internals/src/platform/implementation-notes.md`.
