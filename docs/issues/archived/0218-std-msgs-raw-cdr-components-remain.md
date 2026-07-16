---
id: 218
title: "std_msgs raw-CDR C components remain after phase-293 — workspaces/c, ws-qos-*, mixed templates still hand-encode Int32/String wire bytes"
status: resolved
type: tech-debt
area: examples
related: [issue-0212, rfc-0026]
---

## Problem (phase-293 W4 sweep, 2026-07-16)

Phase-292 retired hand-rolled CDR for the **custom-msg** workspaces (#212),
but the J1 closure grep shows the same antipattern survives wherever the
phase-257 "raw-CDR typed component" convention was applied to STANDARD
messages:

- `examples/workspaces/c/src/c_talker_pkg/src/Talker.c` (+ listener) —
  hand-encoded `std_msgs/Int32` (manual encapsulation header + `buf[8]`).
- `examples/workspaces/ws-qos-c` / `ws-qos-mixed` QoS talker/listener pkgs —
  hand `dds_::` type-name literals.
- `examples/templates/{c-and-cpp-mixed-workspace,pure-c-workspace}` C pkgs.
- `examples/workspaces/ws-realtime-*` C pkgs (same family).

Generated C typesupport exists for std_msgs everywhere (the native C
examples consume it), so these are stale conventions, not capability gaps —
exactly the class phase-293 fixed for custom msgs, and the same J1 audit
violation (examples teach byte-offset wire coding; Architecture §5 says
messages are never hand-written).

## Fix sketch

Mechanical per package (proven shape in phase-293 W2): declare the msg dep
in package.xml (already there in most), include the generated umbrella
header, replace the hand encode/decode with `<pkg>_msg_<type>_serialize` /
`_deserialize` + `_get_type_name()`. Rebuild workspace fixtures + rerun each
workspace's e2e lane. Bigger than #212 only in file count (~8 pkgs).

## Resolution (2026-07-16)

All 15 files migrated onto generated std_msgs bindings (phase-293 W2 shape):

- C talkers (workspaces/c, pure-c-workspace, c-and-cpp-mixed-workspace,
  multi-package-workspace, zephyr-byo): `std_msgs_msg_int32` +
  `_init/_serialize` + `_get_type_name()`; the manual encapsulation header,
  `write_u32_le` helper, and `dds_::` literals are gone.
- C listeners (workspaces/c, ws-qos-c, ws-qos-mixed, pure-c-workspace):
  `_deserialize` replaces the byte-picking; raw callback shape kept (that IS
  the C API), decode is generated.
- C++ listeners (workspaces/cpp, workspaces/mixed, ws-qos-cpp, and the
  c-and-cpp-mixed / multi-package / multi-node-workspace-cpp templates):
  raw `on_raw` + hand decode replaced by RFC-0044 typed
  `bind_subscription<std_msgs::msg::Int32>` member callbacks (QoS-profile
  overload included).

Verified: all six affected workspaces configure+build clean; nextest —
workspace/qos sweep + cpp_multi_node_entry 4/4 (incl. live typed-listener
pubsub e2e) + `c_nuttx_workspace_entry_delivers_cross_process` PASS (the
rewritten workspaces/c delivering from a NuttX QEMU guest cross-process).
zephyr-byo is template-only (no fixture consumes it); the one remaining red
in the sweep (`test_zephyr_workspace_entry_native_sim_e2e`) is a stale
zephyr fixture from unrelated upstream churn, not this migration.

Deliberately NOT migrated (raw path is their subject): the zephyr
service/action hand-CDR examples (240.5 raw-transport demos),
component-poc (RFC-0043 raw POC), custom-platform (platform-port demo).
