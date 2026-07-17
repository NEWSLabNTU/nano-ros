---
id: 227
title: "C/C++ init uses domain_id==0 as the unset sentinel — an image with a baked nonzero domain can never be explicitly run on domain 0"
status: resolved
resolved_in: "2026-07-17 — NROS_DOMAIN_ID_EXPLICIT_ZERO (255) / nros::kDomainIdExplicitZero at the C/C++ init surface maps to an explicit baked domain 0 through the ONE resolver; u8→u32 type unification deferred to the next ABI window (recorded)"
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

## RESOLVED (2026-07-17)

Pure extension, no behavior change for existing callers:

- `nros-node`: `DOMAIN_ID_EXPLICIT_ZERO_C_ABI = 255` + `baked_domain_from_c_abi(u8) -> Option<u32>`
  (0 → unset/None, 255 → Some(0), else pass-through — 233..=254 still hit the
  resolver's `DomainIdRange` error, same as before). Unit-tested incl. the
  embedded path resolving an explicit 0.
- `nros_support_init[_named]` and `nros_cpp_init` route their `u8` argument
  through the shared mapper; the C constant `NROS_DOMAIN_ID_EXPLICIT_ZERO`
  is cbindgen-exported into `nros_generated.h`, and C++ gets
  `nros::kDomainIdExplicitZero` beside `init()` (docs on both overloads +
  the `NROS_ENTRY_DOMAIN_ID` fold note that only 0 folds).
- Model-A semantics preserved (maintainer decision, #206): hosted
  `ROS_DOMAIN_ID` still overrides the explicit-zero argument like any other
  explicit arg; on hosted, `ROS_DOMAIN_ID=0` already reached domain 0 — the
  real gap was embedded/baked images, now covered.

**Deferred (next ABI window):** the `uint8_t` vs `u32` domain type drift
across the C/C++ FFI (init surface, `nros_support_t.domain_id`) vs the
Rust/vtable `u32`. Breaking; fold into whichever phase next reshapes the C
ABI. The sentinel above is forward-compatible with it
(`NROS_DOMAIN_ID_INHERIT`-style `u32::MAX` becomes the natural unset value
and 255 can be retired).
