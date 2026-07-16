---
id: 227
title: "C/C++ init uses domain_id==0 as the unset sentinel — an image with a baked nonzero domain can never be explicitly run on domain 0"
status: open
type: bug
severity: low
area: cpp
related: [issue-0206]
---

## Finding (deep audit 2026-07-17, C6)

`packages/core/nros-cpp/include/nros/node.hpp:~736` — the baked-macro rung
applies via `if (domain_id == 0) domain_id = NROS_ENTRY_DOMAIN_ID;`, so
explicit domain 0 is indistinguishable from "unset" once a nonzero
NROS_ENTRY_DOMAIN_ID is baked. Related: `uint8_t` domain in the C/C++ FFI vs
`u32` everywhere else (Rust config, vtable open(), TopicInfo) — same-concept
type drift.

## Fix sketch

RFC-0045's resolver already models "unset" properly (Option/INHERIT
sentinel = u32::MAX in nros-c node options). Extend that shape to the
init()/support_init surface (e.g. NROS_DOMAIN_UNSET sentinel or an
explicit-args variant) and unify the domain type at u32 in the next ABI
window.
