---
id: 1028
title: "NuttX is classified `hosted` because its `target_os` is not `\"none\"`, so it
  takes the Linux 32-queryable budget: 142,336 B of `.bss` in an image with zero
  queryables"
status: open
type: bug
area: [rmw, memory, build]
related: [0827, 0870, 0460]
---

## What

`nros-zpico-build/src/runner.rs:246` picks the queryable budget for an image
that declares nothing:

```rust
None => return if hosted { 32 } else { UNDECLARED_HEADROOM },
```

`hosted` is `CARGO_CFG_TARGET_OS != "none"` (`runner.rs:96`). NuttX's Rust
target is `armv7a-nuttx-eabihf`, and:

```
$ rustc --print cfg --target armv7a-nuttx-eabihf | grep target_
target_family="unix"
target_os="nuttx"
```

`"nuttx" != "none"`, so an RTOS takes the 32-slot guess written for Linux.

**"Is this hosted?" and "is `target_os` set?" are different questions**, and
NuttX is the counterexample: it is POSIX-ish enough to name itself, and it is
still an RTOS on a part with a fixed RAM budget.

## Measured

`examples/qemu-arm-nuttx/cpp/action-client`, an image that opens **zero**
queryables (an action client declares three service *clients* and one feedback
subscription — no queryable at all):

```
$ nm -S .../build-zenoh/cpp_action_client | grep SERVICE_BUFFERS
40202c88 00022c00 b ..._14nros_rmw_zenoh4shim7service15SERVICE_BUFFERS
```

`0x22c00` = **142,336 bytes** of `.bss` — 32 slots x 4,448 B. The same table at
the embedded budget (8) would be 35,584 B. **106,752 B wasted.** Byte-identical
in the C image.

The array is `SERVICE_BUFFERS` (`shim/service.rs:108`), sized
`ZPICO_MAX_SESSIONS * ZPICO_MAX_QUERYABLES` at `:107`.

## Why it has not bitten

`nros-board-nuttx-qemu` has `CONFIG_RAM_SIZE=132120576` (126 MB), so on QEMU
this is waste rather than failure. It is not waste on a real NuttX part, and
the budget is decided at build time by a predicate that does not know which it
is running on.

## Not the cause of #0870

This was found while investigating #0870 (NuttX C++ action client fails
`create_action_client`) and is **not** its cause — the pools are large enough,
and more importantly pool exhaustion cannot produce that issue's error code at
all. Recorded separately so it does not stay buried inside a killed lead.

## Fix direction

Do not widen `hosted`; it is used elsewhere for genuinely POSIX questions.
Narrow this one call site: an RTOS `target_os` (`nuttx`, and check `espidf`,
`horizon`, and anything else in-tree) takes `UNDECLARED_HEADROOM` regardless of
whether it names itself.

The deeper fix is the one phase-392 W5 already names as Open: **every image
declares its entities**, and the fallback budget stops mattering. Zephyr
already derives per-image from the entity inventory
(`zephyr/Kconfig:694` default `-1` -> `nros_cargo_build.cmake:406`); NuttX
never got that treatment.

## Confirm cheaply

`nm -S <elf> | grep SERVICE_BUFFERS` before and after. No run needed.
