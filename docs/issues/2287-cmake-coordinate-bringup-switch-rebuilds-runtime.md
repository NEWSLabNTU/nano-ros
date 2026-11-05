---
id: 2287
title: "Images of different bringups share one cmake coordinate, and every bringup switch rebuilds the Rust runtime"
status: open
type: perf
area: [build, cmake, cli]
severity: medium
found: 2026-11-05
related: [phase-535, RFC-0122, RFC-0100]
---

**Home: [phase-535](../roadmap/phase-535-example-reorg-and-cadence-ci.md) W3.**
The fixture build's critical path on the phase-535 pilot is one cmake
configure that several bringups take turns writing.

## Measured

`examples/workspaces/c` on the pilot (#3129) has seven `mps2-an385-freertos`
images in four bringups (`demo`, `pubsub`, `service`, `action`). `nros build`
puts all seven in ONE build dir, `build/freertos-zenoh-mps2-an385-freertos/`,
because the coordinate is `(platform, rmw, board, configuration)` and names no
bringup. The generated root it writes there, though, is per BRINGUP: it
carries the `nano_ros_add_executable` calls of the current bringup's images
only, and `nano_ros_workspace(SYSTEM <bringup>)`.

So building the seven rows is four configures in turn. Timed on a built tree,
2026-11-05:

| step | wall |
| --- | --- |
| `nros build pubsub_bringup:freertos_talker`, current | 4 s |
| both pubsub images in one `nros build`, current | 9 s |
| switch to `service_bringup:…` | 22 s |
| switch back to `pubsub_bringup:…` | 23 s |
| switch to `service_bringup:…` again | 32 s |

Every switch recompiles `nros-params`, `nros-rmw-cffi`, `zpico-sys`,
`nros-node`, `nros`, `nros-c`, `nros-cpp`, `nros-rmw-zenoh` and the two
`*-image` crates. The cargo command's environment names
`NROS_SIZING_DESCRIPTOR`, and the descriptor a configure composes covers the
CURRENT bringup's images (RFC-0100 D12's shared runtime). A different bringup
states different entity counts, so the descriptor moves and so does every
crate that reads it. Within one bringup there is no such cost: the untargeted
`cmake --build` already builds every entry of the root in one ninja graph.

In the pilot's fixture build that is three switches per C/C++ unit, about
130 s of serial work after the first image's ~205 s
([data](../roadmap/data/phase-535-w3-pilot.md)).

## Why the fixture builder cannot fix it

Phase-535's lever "one `cmake --build` over every entry of a configure" assumed
the seven rows were seven targets of one root. They are not: the root is
rewritten per bringup, and its one `SYSTEM` decides the capability axes
(`nros config show --system`) that the C/C++ libraries are built with. Running
one `nros build` per bringup with all its images was measured too: the
per-image cost is the same in one process as in several (4 s versus 9 s for
two), so it saves nothing.

## Options

1. One root per coordinate that carries the entries of EVERY bringup that
   resolves to it, with the runtime descriptor composed over all of them. The
   open question is `SYSTEM`: the capability axes would have to be the union
   (or the coordinate would have to key on them, so bringups that disagree get
   different dirs).
2. Key the coordinate on the bringup as well. Each bringup then gets its own
   build dir and runtime; the switches disappear, and the four bringups become
   four build units the fixture builder already runs at once. This costs a
   runtime build per bringup instead of one per configuration.
3. Leave it. It is a cost of the fixture lane, where several bringups share a
   board; a user building one bringup never pays it.
