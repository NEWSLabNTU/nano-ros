---
id: 218
title: "std_msgs raw-CDR C components remain after phase-292 — workspaces/c, ws-qos-*, mixed templates still hand-encode Int32/String wire bytes"
status: open
type: tech-debt
area: examples
related: [issue-0212, rfc-0026]
---

## Problem (phase-292 W4 sweep, 2026-07-16)

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
exactly the class phase-292 fixed for custom msgs, and the same J1 audit
violation (examples teach byte-offset wire coding; Architecture §5 says
messages are never hand-written).

## Fix sketch

Mechanical per package (proven shape in phase-292 W2): declare the msg dep
in package.xml (already there in most), include the generated umbrella
header, replace the hand encode/decode with `<pkg>_msg_<type>_serialize` /
`_deserialize` + `_get_type_name()`. Rebuild workspace fixtures + rerun each
workspace's e2e lane. Bigger than #212 only in file count (~8 pkgs).
