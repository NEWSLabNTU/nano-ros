---
id: 206
title: "ROS_DOMAIN_ID/NROS_LOCATOR env overlay lives only in the C++ header — C diverges, logic duplicated, malformed input silently becomes domain 0"
status: resolved
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

## RESOLVED — 2026-07-16

Lifted the overlay into the shared core (fix sketch as filed):

- **nros-c (`support.rs`)** exports two hosted-only helpers —
  `nros_env_locator()` (`$NROS_LOCATOR`, cached, NULL when unset) and
  `nros_env_domain_id()` (`0..=232` valid / `-1` unset / `-2` malformed or
  out of range — the caller keeps its value, never silent domain 0) — plus
  the named `NROS_DOMAIN_ID_MAX = 232` const. Declarations added to the
  committed `nros_generated.h` (hand-placed; the header carries manual C23
  guards, so no wholesale cbindgen regen).
- **C parity:** `nros_support_init_named` applies the overlay when
  `locator == NULL` / `domain_id == 0` — explicit arguments always win, so C
  apps that already `getenv()` themselves are unchanged.
- **C++ dedup:** both `init()` overloads' ~18-line inline blocks collapse to
  `detail::apply_env_overlay(locator, domain_id)` calling the shared helpers;
  the silent `acc = 0; break` parser is gone.

Verified: `cargo build/clippy -p nros-c` clean; a freshly-built native C++
talker with `NROS_LOCATOR` env + a MALFORMED `ROS_DOMAIN_ID` connects via the
env locator and publishes (8 samples — the typo no longer moves the node);
`c_parameters_roundtrip` + `cpp_parameters_roundtrip` stay green.
