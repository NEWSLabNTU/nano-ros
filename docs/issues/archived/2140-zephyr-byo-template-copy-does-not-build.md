---
id: 2140
title: "A copy of `zephyr-byo/app` does not build with `nros build`: `Talker.c` includes `std_msgs/std_msgs.h`, which nothing in the app generates"
status: resolved
resolved_in: "2026-10-29"
type: bug
area: [examples, zephyr]
severity: low
found: 2026-10-29
related: [1782, phase-452]
---

**Home: [phase-452](../../roadmap/archived/phase-452-copy-out-front-door.md)** — found by
the copy-out gate the day issue 1782 gave it a Zephyr road.

## What was measured

Until issue 1782, `check-template-copy-out` reported every Zephyr sub-project
NOT VERIFIED, so `examples/templates/zephyr-byo/app` was never built from a
copy. With the Zephyr arm now building (`nros sync`, then `nros build
--zephyr-workspace <ws>` in the copied leaf), the four port templates build
and this one fails, against `~/.nros/workspaces/zephyr/3.7`:

```
src/Talker.c:19:10: fatal error: std_msgs/std_msgs.h: No such file or directory
```

(`west build -b native_sim/native/64`, the board `[image.zephyr] board =
"zephyr"` lowers to.) `nros sync` in the copy exits 0 first, so this is not
the "never synced" refusal.

## Why

`app/CMakeLists.txt` says the app needs no generated message headers ("The
raw publisher carries the type name as a string"), and calls neither
`find_package(nano_ros)` nor `find_package(std_msgs)`. `src/Talker.c` was
since rewritten against `std_msgs__msg__Int32` and
`ROSIDL_GET_MSG_TYPE_SUPPORT(std_msgs, msg, Int32)`, so the comment and the
build no longer describe the source. `examples/zephyr/c/talker` is the shape
that builds (`find_package(nano_ros)` + `find_package(std_msgs)`).

No fixture row builds this template, which is how it drifted.

## Consequence

`check-template-copy-out` reports `zephyr-byo/app` NOT VERIFIED, naming this
issue, so the lane stays green for the templates that do build. A user who
copies the template gets the compile error above.

## Direction

Make the app's build match its source (the `examples/zephyr/c/talker` shape,
plus the `std_msgs` dependency in `package.xml`), then drop the gate's
`KNOWN_UNBUILDABLE` row for it. Its README's road (`west init -m
nano-ros-zephyr-example`, the NSOS overlay) is a second question: decide
whether `nros build` in the copy is a supported road for this template at
all, and say so in the README.

## Resolution (2026-10-29)

Two defects, the second hidden behind the first.

1. **The build did not declare what the source includes.** `app/CMakeLists.txt`
   now takes the `examples/zephyr/c/talker` shape (`find_package(nano_ros)` +
   `find_package(std_msgs)`) and `package.xml` declares `<depend>std_msgs</depend>`.
   A copy then compiles and links `zephyr.exe`.
2. **The image it built SEGFAULTed at boot.** With nothing selecting the native_sim
   offloaded sockets, Zephyr picked the TAP driver
   (`<err> eth_posix: Cannot create zeth (0)`, then a core dump). Every in-tree
   native_sim leaf carries `boards/native_sim_native_64.conf` (18 identical
   copies under `examples/zephyr/`), which Zephyr merges itself on the `nros
   build` road; this app did not. It does now.

Measured on a copy made outside the repo, against
`~/.nros/workspaces/zephyr/3.7`: `nros sync && nros build` exits 0, and the
image, run beside `ros2 run rmw_zenoh_cpp rmw_zenohd` listening on 7447,
printed `Published: 0` … `Published: 10` in 12 s. The README now names `nros
build` as a supported road beside its `west build` one, and its router line and
`Talker.c`'s comment say 7447 (the Kconfig default), not 7456.

`check-template-copy-out`'s `KNOWN_UNBUILDABLE` row for this app is gone; the
table is empty.

Not done here: the 18 copies of `boards/native_sim_native_64.conf` name
`CONFIG_ETH_NATIVE_POSIX`, a Zephyr 3.7 symbol (4.4 renamed it
`ETH_NATIVE_TAP`, which `cmake/zephyr/native-sim-line-4.4.conf` handles for the
fixture lanes). Folding them into the generic `zephyr` board package, the way
issue 1782 folded the mps2 tail, needs a per-line choice the board-files rule
does not have.
