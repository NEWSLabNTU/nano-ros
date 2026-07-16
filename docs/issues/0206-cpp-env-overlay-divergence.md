---
id: 206
title: "ROS_DOMAIN_ID/NROS_LOCATOR env overlay lives only in the C++ header — C diverges, logic duplicated, malformed input silently becomes domain 0"
status: open
type: bug
severity: low
area: cpp
related: []
---

## Problem (audit 2026-07-16, C1/I1/I3)

`packages/core/nros-cpp/include/nros/node.hpp:686-760`:

- The env→locator/domain resolution (getenv + 0-232 parse/clamp) is business
  logic in the C++ shim only; `nros-c` forwards raw values with no env
  fallback — C and C++ apps behave differently under the same environment.
- The ~18-line block is duplicated verbatim across the 2-arg and 3-arg
  `init()` overloads; the 232 domain max is an inlined literal.
- A malformed or >232 `ROS_DOMAIN_ID` silently collapses to domain 0
  (`acc = 0; break`) — a typo invisibly moves the node to domain 0.

## Fix sketch

Lift the overlay into the shared Rust core (or one FFI helper both shims
call); single named DOMAIN_MAX const; parse failure keeps the caller value or
surfaces an error — never silent 0.
