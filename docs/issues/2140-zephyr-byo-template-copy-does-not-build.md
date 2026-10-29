---
id: 2140
title: "A copy of `zephyr-byo/app` does not build with `nros build`: `Talker.c` includes `std_msgs/std_msgs.h`, which nothing in the app generates"
status: open
type: bug
area: [examples, zephyr]
severity: low
found: 2026-10-29
related: [1782, phase-452]
---

**Home: [phase-452](../roadmap/phase-452-copy-out-front-door.md)** — found by
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
