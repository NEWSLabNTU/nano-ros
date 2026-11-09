---
id: 2377
title: "The workspace road cannot build a C or C++ image for a board with no RTOS, so the bare-metal C/C++ pairs stay leaves"
status: open
type: limitation
area: [build, cli, cmake]
severity: low
found: 2026-11-09
related: [phase-535, rfc-0122, 1512, 2376]
---

**Home: [phase-535](../roadmap/phase-535-example-reorg-and-cadence-ci.md).**

Issue 2376 states the same fact and records the user's ruling on it:
C/C++ is refused on bare metal. `nros new --lang c|cpp` refuses there and
names mps2-an385-freertos, and the Tutorial's C/C++ tabs move to FreeRTOS.
Follow that ruling here; the fix direction below is the reasoning before it.

## What happens

Phase-535 W5 moves each platform's example pairs into the language
workspaces' behaviour bringups (RFC-0122 D1). On `mps2-an385-baremetal` that
works for Rust. For C it fails at configure. A
`[image.baremetal_talker]` with `board = "mps2-an385-baremetal"` in
`examples/workspaces/c/src/pubsub_bringup/system.toml`, built with
`nros build pubsub_bringup:baremetal_talker`, stops in
`cmake/NanoRosEntry.cmake`:

```
codegen entry-pack: unknown board key `mps2-an385-baremetal` — the
entry pipeline knows: linux, zephyr, …, rv-virt-threadx
```

## Why

A C/C++ workspace image goes to the cmake driver, which generates a C `main`
entry. On a board with no RTOS that is the wrong link root. The startup code,
the clock and the LAN9118/smoltcp bring-up are Rust. Issue 1512 solved this
for the leaves by making each one a cargo package: `src/main.rs` boots and
calls `app_main()`, and `build.rs` compiles the C/C++ application. The
C-rooted road was deliberately not pursued (1512, D4). The workspace road
has no equivalent: no generated cargo entry links a C/C++ component package.

## Consequence

The twelve `examples/mps2-an385-baremetal/{c,cpp}/*` leaves keep their rows,
and `test_qemu_baremetal_capi_e2e` keeps resolving them. Nothing is lost.
They are the six `(QemuBaremetal, C|Cpp, Zenoh, *)` Runtime cells' only
images.

## Fix direction

Do one of two things:

- Have the workspace road route a C/C++ image on a board with a Rust link
  root (`mps2-an385-baremetal`) to a generated cargo entry, which compiles
  the bringup's C/C++ component packages as 1512's `build.rs` does.
- Keep these as the board's manual-entry demos (RFC-0122 D10), with this
  issue as their written reason.
