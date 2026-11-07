---
id: 2287
title: "Images of different bringups share one cmake coordinate, and every bringup switch rebuilds the Rust runtime"
status: resolved
type: perf
area: [build, cmake, cli]
severity: medium
found: 2026-11-05
related: [phase-535, RFC-0122, RFC-0100, RFC-0065]
resolved: 2026-11-07
resolved_in: "fix-issue-2287 branch: the cmake build dir names the bringup"
---

**Home: [phase-535](../../roadmap/phase-535-example-reorg-and-cadence-ci.md) W3.**
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
([data](../../roadmap/data/phase-535-w3-pilot.md)).

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

## Resolution

Option 2, ruled by the user on 2026-11-07: the cmake coordinate names the
bringup as well. The reason given: a user can keep two bringup packages for
one board, with different launch files and settings, and today every switch
between them rebuilds the Rust runtime. With one build dir per bringup, each
bringup has its own root and its own runtime, and a switch rebuilds nothing.

`cmd::build::cmake_coordinate` is now the configuration key (the old
coordinate, `cmake_configuration_key`: platform, rmw, board, `-cfg<hash>`)
followed by `-<bringup>`. A bringup is a ROS package name, so it needs no
escaping. Every reader takes the path from that one function: the generated
root and its `cmake/` binary dir, `ResolvedBuild::cmake_build_dir` (which
`nros run`'s artifact, `nros image-facts` and the fixture-row check read), and
the issue-1582 output-collision check. `nros clean` removes `generated/` and
the managed patches only, and names no build dir.

The layout changes for every CMake workspace, single-bringup ones included:
`build/posix-zenoh-linux/cmake` is now
`build/posix-zenoh-linux-<bringup>/cmake`. Suffixing only when a second
bringup shares the board would have kept a single-bringup path, but adding a
bringup would then move the first one's dir, and the planner would need the
whole workspace to name one image's dir. The old dir is left in place; a
`rm -rf build/` reclaims it.

Updated with it: the 141 `build_subdir` rows of `examples/fixtures.toml` (the
CLI test `every_generated_workspace_fixture_row_names_its_cmake_build_dir`
checks them), the template `compile_check_fixture` outputs, the
`nros-tests` locators that restate a row's dir, the probe verifiers, and the
book pages and READMEs that print the path. RFC-0065 D8 records the amended
layout.

A consequence for issue 1582: one image id in two bringups on one board was a
cmake collision (one target in one root). It is two roots now, so it is not
refused on the cmake road; the cargo and west roads still key their generated
entry on the id and still refuse it.

Test: `two_bringups_on_one_board_never_share_a_cmake_build_dir` in
`packages/cli/nros-cli-core/tests/build_verb_pipeline.rs`. It plans
`demo_bringup:linux`, then `other_bringup:other_linux`, then the first again,
and asserts the first one's generated root keeps its bytes and its mtime
throughout. Before the change both planned `build/posix-zenoh-linux/cmake`
and the assertion failed; after, the two dirs differ and the first root is
untouched.

Measured with this branch's CLI on a scaffolded C workspace
(`nros new two_ws --board linux --lang c`) plus a copy of its bringup,
`alt_bringup` with `[image.linux_alt]`, both on `linux`, 2026-11-07, loaded
host:

| step | wall |
| --- | --- |
| `nros build two_node_bringup:linux`, first | 21.7 s |
| `nros build alt_bringup:linux_alt`, first | 41.2 s |
| switch back to `two_node_bringup:linux` | 0.39 s |
| switch to `alt_bringup:linux_alt` again | 0.39 s |

Each bringup built into its own dir (`build/posix-zenoh-linux-two_node_bringup/`
and `build/posix-zenoh-linux-alt_bringup/`). Every `libnros_c*.a`,
`libnros_cpp*.a` and `*_entry` under the first one kept its mtime across the
other bringup's build and the switch back. The issue's own table measured
22 to 32 s per switch before the change; the before run was not repeated here.
