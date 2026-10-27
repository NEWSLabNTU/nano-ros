---
id: 2098
title: "A posix Rust binary fails to link with `undefined reference to nros_platform_clock_ns` when nothing above nros-log pulls the port"
status: resolved
type: bug
area: [platform, posix, build, log]
severity: medium
found: 2026-10-27
related: [1776, 1779, 1037, 0710]
resolved: 2026-10-27
resolved_in: "the PR that archives this file"
---

## What

`just build-test-fixtures lane=linux` stopped at the `log-arena-probe` row:

```
/usr/bin/ld: .../libnros_log-*.rlib(...): in function `nros_log::__timestamp_ns':
  packages/core/nros-log/src/lib.rs:1145: undefined reference to `nros_platform_clock_ns'
error: could not compile `log-arena-probe`
```

Every row queued behind it in the lane was left unbuilt. A later test run then
read their missing binaries as `[UNMET PRECONDITION] fixture not built` (43 cells
of `linux_api` in one run). It reproduces on main with `nros build` in the leaf.
A plain `cargo build` of the leaf links, because without the image's generated
config `nros-log` does not compile its clock (`nros_log_clock` comes from the
platform's `clock` capability, which only the image road states).

## Cause

A symbol cycle between two link positions:

- `nros-platform-cffi` depends on `nros-log` (the platform sink lives in cffi,
  issue 0710), and on `nros-platform-api`. rustc orders a crate before its
  dependencies, so cffi's rlib, and the POSIX port archive `posix-c-port` bundled
  INTO it, sit before `libnros_log` and `libnros_platform_api` on the link line.
- `nros-log` calls `nros_platform_clock_ns`, and `nros-platform-api` calls the
  task, yield and stack-probe symbols. All of them are defined in the port's
  `platform.c`.

GNU ld scans each archive once. When nothing before the port has referenced a
`platform.o` symbol, the member is skipped, and the later references stay
undefined. Real images linked only by accident: an executor, an RMW or the
platform sink happened to pull `platform.o` first. `log-arena-probe` has none of
them.

## Fix

`nros-platform-cffi/build.rs`, under `posix-c-port`:

- compiles `c/port_anchor.c`, one object holding a reference to
  `nros_platform_clock_ns`, and links it `+whole-archive`;
- keeps the port BUNDLED, which is what puts it inside a staticlib built from
  the graph (`libnros_c.a`);
- links a byte-identical TAIL copy of the port `-bundle`, which rustc places
  right after the anchor. The bundled copy sits inside cffi's rlib, ahead of
  the anchor, so it cannot answer the anchor's reference.

The anchor's reference is undefined by the time the tail is scanned, so
`platform.o` is pulled there, still ahead of `nros-log`. Every symbol the lower
crates call is in that member, so the one reference covers the class. Both
copies are demand-driven with identical members, so no symbol is defined
twice. A staticlib does not carry a `-bundle` lib, so it holds the port once.

A first version linked the port `-bundle` only. That dropped it from
`libnros_c.a`, and `check-staticlib-symbols` and `check-c` failed with every
`nros_platform_*` undefined. That is why there are two copies.

Not `+whole-archive` on the port, the other half of the NuttX idiom. The port
must stay demand-driven: `nros-rmw-xrce-cffi` compiles its own copy of the same
sources, which coexists only because neither copy is whole-archived (issue
1779), and `termination.c` is kept as its own member for issue 1776. Not
`-bundle` alone either: current rustc places a `-bundle` lib right after its
own crate, not after every rlib as the NuttX build's comment expects, so on its
own it moves nothing.

## Measured

On the fixed tree, `aarch64` Linux:

- `log-arena-probe` links through `nros build` and through its fixture row, and
  prints `clock=1`. Its test `log_arena_knob` passes (2/2). On main the same
  build fails with the reference above.
- `examples/linux/{c,cpp,rust}/talker` and the `linux_xrce` image of
  `examples/linux/rust/action-client` all link. The XRCE image carries both
  copies of the port.
- `nros-platform-cffi --features posix-c-port` (45), `nros-log` (48) and
  `nros-node` (759) tests pass.
