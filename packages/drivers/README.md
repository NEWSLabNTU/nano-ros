# `packages/drivers/`

Hardware and kernel-facing support crates, grouped by what they talk to.

Before phase-321 W2.f this was a flat bag of fifteen entries mixing five
unrelated things, with **no README and no documented taxonomy** — and CLAUDE.md
pointed at RFC-0012 for a "category split" that RFC-0012 does not define. So
"does my crate belong here?" had no answer. It does now.

## Categories

| Directory | Holds | Members |
| --- | --- | --- |
| `net/` | MAC/PHY drivers and the network-stack adapters they feed | `lan9118-smoltcp`, `openeth-smoltcp`, `lan9118-lwip` (C), `virtio-net-netx` (C), `nros-smoltcp`, `nsos-netx` (C) |
| `serial/` | UART peripherals | `cmsdk-uart`, `stm32f4-usart` |
| `ipc/` | Inter-processor channels | `nvidia-ivc` |
| `sys/` | `-sys` FFI bindings to a KERNEL or vendor stack — not to hardware | *(none — see below)* |

`sys/` is its own category on purpose: those crates bind a kernel or a vendored
network stack, so they behave like platform glue rather than device drivers, and
lumping them with real drivers is what made the flat list unreadable.


### `sys/` is currently empty, and that is a statement, not an oversight

It held four bindgen crates — `freertos-lwip-sys`, `nuttx-sys`,
`threadx-netx-sys`, `zephyr-posix-sys` — deleted 2026-08-30 (phase-400). All four
were `exclude`d from the workspace (each needs an SDK env var to build) and had
ZERO consumers anywhere in the tree: no manifest depended on them, no recipe built
them, and the only lockfiles resolving them were their own. Nothing was measured
to get faster by removing them; they cost nothing because nothing built them.
They were deleted because a crate nobody builds and nobody uses is a false
statement about what the project needs.

The CATEGORY stays documented: a kernel/vendor-stack binding belongs here rather
than in `net/`, and the distinction is the reason for the split (phase-321 W2.f).
If one is added, wire it to a consumer in the same change — that is the check
these four never had to pass.

## What was moved OUT, and why

Two crates were never drivers, and each said so in its own first doc line:

- **`nros-baremetal-common` → `packages/platform/`** — *"Shared bare-metal
  helpers for nros-platform-\* crates."* It supports platform ports; it drives
  nothing.
- **`nros-transport-callbacks` → `packages/rmw/transport-callbacks`** —
  *"reusable custom-transport callback factories"* for
  `nros_rmw::NrosTransportOps`. That is the RMW transport vtable, so it belongs
  with the RMW layer.

## Judgement calls worth knowing

`nros-smoltcp` and `nsos-netx` are **not** hardware drivers either —
`nros-smoltcp` is a TCP/UDP provider over the smoltcp stack, and `nsos-netx` is
a BSD-sockets-over-POSIX compat shim for ThreadX-on-Linux. They are kept under
`net/` because they sit directly beneath the MAC drivers that feed them, and
splitting them off would separate things that are read together. If a future
`packages/net/` group appears, they are the first candidates to move.
