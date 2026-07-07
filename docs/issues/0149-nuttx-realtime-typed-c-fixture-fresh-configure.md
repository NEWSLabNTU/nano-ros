---
id: 149
title: "phase-281 nuttx-realtime workspace fixtures fail from a fresh configure — generated C interface headers never materialize before the cargo kernel build"
status: open
type: bug
area: nuttx
related: [phase-281, issue-0136]
---

## Summary

The two phase-281 W3-nuttx workspace fixtures (`workspace-c-nuttx-realtime`,
`workspace-cpp-nuttx-realtime`; `examples/workspaces/ws-realtime-{c,cpp}`
nuttx lanes) fail to build from a FRESH configure — reproduced twice with the
canonical `just nuttx build-examples` after wiping
`build-workspace-fixtures-nuttx`:

```
ctrl_pkg/src/Ctrl.c:16:10: fatal error: std_msgs.h: No such file or directory
error: failed to run custom build command for `nros-nuttx-ffi v0.4.0`
```

Three symptoms, one cause — the entry carrier's `LINK_INTERFACES` walk comes
up empty for the generated interface libs:

1. `<build>/src/ctrl_pkg/nano_ros_c/std_msgs/` contains only the empty
   `action/ msg/ srv/` skeleton — the `<pkg>__nano_ros_c_gen` custom command
   never runs before the cargo kernel build (missing dependency edge).
2. `nuttx_entry_includes.txt` (the `file(GENERATE)` include closure handed to
   the FFI cc-rs build) lists only static dirs — no generated
   `nano_ros_c/...` include dir.
3. `APP_INTERFACE_SOURCES=` is empty, so the serdes TUs (`std_msgs_msg_int32.c`)
   that phase-281 W3-nuttx routes into the trailing `app_iface` archive are
   never compiled either.

`cmake/board/nano-ros-board-nuttx-qemu-arm.cmake` builds `_link_ifaces` from
`get_target_property(${target} LINK_LIBRARIES)` on the entry carrier; the
`std_msgs__nano_ros_c` lib is linked to the NODE component libs
(`ctrl_pkg`/`telem_pkg` CMakeLists), and whatever sidecar is expected to also
attach it to the entry carrier isn't doing so on a fresh configure.

This is the first NuttX workspace that uses `nros_find_interfaces` + typed C
nodes (the pre-existing `workspace-c-nuttx` chatter fixture is pure-C with no
generated serdes and builds fine), so the path had no prior coverage. The
phase-281 e2e presumably passed against incrementally-built state — same
latent-fresh-configure class as the phase-277 stale component-target names
(fixed cce254ffd).

Until fixed, `just build-test-fixtures`' staleness gate hard-fails on the two
missing fixtures on any machine that hasn't built them; `realtime_tiers_
{c,cpp}_nuttx_e2e` cannot run.

## Repro

```
rm -rf examples/workspaces/ws-realtime-c/build-workspace-fixtures-nuttx \
       examples/workspaces/ws-realtime-cpp/build-workspace-fixtures-nuttx
just nuttx build-examples   # fails: std_msgs.h not found in Ctrl.c
```

Also note: `just nuttx build-fixtures` does NOT build the workspace lanes at
all (they live in `build-examples`) — the fixtures.toml entries have no
recipe home in the fixture verb.
