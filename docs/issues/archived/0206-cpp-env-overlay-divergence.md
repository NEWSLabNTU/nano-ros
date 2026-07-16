---
id: 206
title: "ROS_DOMAIN_ID/NROS_LOCATOR env overlay lives only in the C++ header — C diverges, logic duplicated, malformed input silently becomes domain 0"
status: resolved
resolved_in: "2026-07-16 — env overlay unified into ExecutorConfig::try_resolve (RFC-0045 model A) across Rust/C/C++; malformed or >232 ROS_DOMAIN_ID is an init error"

type: bug
severity: low
area: cpp
related: [rfc-0045, rfc-0004]
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

## RESOLVED (2026-07-16) — RFC-0045 env-rung completion

> Note: a parallel session resolved this the same day via nros-c helper
> functions (`nros_env_locator`/`nros_env_domain_id`, a0061e36e) with
> explicit-args-win semantics. That approach was superseded within hours by
> this resolver-based fix: the maintainer's recorded decision is model A
> (env > explicit args, ROS convention), and RFC-0045's design-of-record is
> ONE resolver, not per-language helpers. The helpers + their hand-added
> `nros_generated.h` declarations were removed in the superseding commit.

The fix followed the existing design-of-record (RFC-0045, precedence
model A: hosted env > baked overlay > compiled default) rather than a new
mechanism — the C++ header blocks were a parallel, divergent
implementation of the env rung that predated the resolver.

- **Resolver hardened** (`nros-node::ExecutorConfig::try_resolve`, new
  fallible twin of `resolve`): malformed `ROS_DOMAIN_ID` = error (it was
  silent-0 in C++ AND silently ignored in the resolver itself — even Rust
  disagreed with itself); any domain (env or BAKED, both paths) above the
  new named `DOMAIN_ID_MAX = 232` = error; `NROS_NODE_NAME` joins the
  hosted env rung (model A parity). `resolve` keeps its infallible
  signature for the boards (panics fail-loud on invalid config).
- **C++**: both duplicated `node.hpp` env blocks deleted (~40 lines of
  header business logic); the header now assembles only the baked rung
  (explicit arg > `NROS_ENTRY_*` macro > hosted default) and
  `nros_cpp_init` routes it through `try_resolve`. Semantic change (per
  the maintainer's model-A decision): on hosted targets the env now
  OVERRIDES an explicit `init(locator, domain)` argument — matching Rust
  and the ROS 2 `ROS_DOMAIN_ID` convention. `domain_id == 0` stays the
  unset sentinel at the ABI edge.
- **C**: `nros_support_init[_named]` gains the env overlay for the first
  time via the same `try_resolve` call (locator + domain; the historical
  per-backend default locator is preserved as the baked rung; the
  XRCE session-name PID default is deliberately NOT routed through the
  resolver's "node" default — session-key collision semantics).
- 6 new resolver unit tests (malformed/range/max-valid/env-over-baked/
  node-name rung/embedded-path validation).
