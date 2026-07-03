# examples/threadx-linux — ThreadX Linux simulation (x86_64)

C, C++ and Rust examples on the ThreadX Linux port: the RTOS runs as a host
process using the NSOS host-kernel sockets shim — no QEMU. Just module:
**`threadx_linux`** (`just/threadx-linux.just`).

## Prerequisites

```sh
source ./activate.sh          # exports THREADX_DIR / NETX_DIR
just threadx_linux setup      # = nros setup threadx-linux + host deps
```

## RMW selection

Effectively board/C-port-driven zenoh: the Rust examples expose
`rmw-{zenoh,xrce,cyclonedds}` features but they are **inert build-target
markers** (the ThreadX C port sets the backend). zenoh is the supported
backend (rust cyclonedds pending — see the [coverage matrix](../README.md)).

## Build & run one example

```sh
just threadx_linux build-examples        # rust fixtures + c/cpp entries
just threadx_linux zenohd &              # router
just threadx_linux talker                # build + run rust/talker (host ELF)
just threadx_linux listener              # peer in a second shell
```

Test lanes: `just threadx_linux test`, `test-all`, `test-c-port`.

## Cases

| Role | c | cpp | rust |
| --- | --- | --- | --- |
| talker / listener | yes | yes | yes (+`_entry`) |
| service-server / service-client | yes | yes | yes (+`_entry`) |
| action-server / action-client | yes | yes | yes (+`_entry`) |

`<case>_entry` = Entry-pkg siblings (`nros::main!()`); underscore naming is an
interim exception (RFC-0026, phase-275). `rust/dds/` is build support, not a
case.

## Gotchas

- Build skips with a message if ThreadX/NetX headers are absent — run setup.
