---
id: 329
title: "C++ headers carry runtime policy and wire decoding: the bounded-spin loop exists 4× with diverged behavior, init() re-implements the RFC-0045 ladder, and action_client hand-decodes the goal-accept payload"
status: open
type: bug
severity: medium
area: core
related: [issue-0226, rfc-0045]
---

## Finding (audit 2026-07-28, P2)

Fresh instances of the C1 rule (the C/C++ user API is a THIN shim over the Rust
core — no logic, no state machines, no duplicated behavior) and of the #226 class
specifically.

### 1. The bounded-spin policy lives in the header, four times

`packages/core/nros-cpp/include/nros/main.hpp:103` — `component_spin_loop()`
implements the whole policy in the header: the `NROS_ENTRY_SPIN_MS` env read (via
a hand-rolled `entry_parse_u32` at :76), wall-clock budgeting, and the
cooperative yield.

The same loop exists at `nros-cpp/src/lib.rs:754`, `:2215`, and
`include/nros/executor.hpp:139` — **four copies, already diverged**: only the
header copy checks `nros::ok()`.

Fix: export one `nros_cpp_spin_bounded(storage, bound_ms)` from the CFFI with the
env rung resolved Rust-side; all four call sites forward to it.

### 2. `init()` re-implements the config ladder in the header

`packages/core/nros-cpp/include/nros/node.hpp:710-769` resolves the baked-macro
and hosted-default rungs of the RFC-0045/#206 ladder in the header, duplicated
verbatim across both overloads (the 2-arg resolves, then calls the 3-arg which
resolves again). The ladder is therefore split between the shim and the Rust
resolver — two places to change, one of which is a header users compile.

Fix: pass locator/domain through unset and resolve every rung in the Rust
resolver behind `nros_cpp_init`; the header supplies only the `-D NROS_ENTRY_*`
values.

### 3. `action_client.hpp` hand-decodes the wire

`packages/core/nros-cpp/include/nros/action_client.hpp:82` —
`GoalAccept::ffi_deserialize` decodes the goal-acceptance payload by hand
(goal_id at byte 0, `accepted` at byte 16) inside a public C++ header, with a
magic `SERIALIZED_SIZE_MAX = 32` for a 17-byte payload. Wire decoding belongs to
the codegen/serdes layer that owns it for every other type — this is #226's
shape (logic in the C++ shim that the Rust core already owns), and it is a
layout dependency that no ABI assert covers.

Fix: have the CFFI return a generated envelope type, or already-split
`(uuid, accepted)` out-params; delete the header-side decoder.

## Why grouped

One rule (C1), one surface (`nros-cpp/include`), and one fix direction (move the
behavior behind the CFFI, leave the header adapting types only). #226 established
the precedent for this shape of issue.

## Addendum (deep audit C,E 2026-07-28) — one of the four copies carries a known-fixed bug

`packages/core/nros-cpp/include/nros/nros.hpp:109` — the global
`nros::spin(duration_ms, poll_ms)` budgets by **iteration count** (`elapsed +=
timeout`), which is the exact defect `Executor::spin` documents as fixed in Phase
118.C. An early `nros_cpp_spin_once` return (signalled wake condvar) therefore exits
the loop long before `duration_ms` of wall time has passed.

So the duplication above is not merely a maintenance cost: the copies have already
diverged, and one of them still has the bug the other fixed. That makes the fix
direction non-optional — move the budgeted spin behind ONE CFFI entry point
(`nros_cpp_spin_for(storage, duration_ms, poll_ms)`) and have the free function, both
`Executor` overloads, and the `src/lib.rs` copies all forward to it. Naming/semantics
of the public verb are tracked separately in **#338**.

## Progress (2026-07-28)

**Addendum bug — FIXED (1a64eeb45).** Added `nros_cpp_spin_for(handle,
duration_ms, poll_ms)` — the single wall-clock-budgeted spin, Rust-side.
`nros::spin()` (nros.hpp, the iteration-count-budgeting copy with the latent
bug) and `Executor::spin()` (executor.hpp) now both forward to it. The two
duration-spin copies are deduped and the collapse-to-milliseconds bug is gone.

**Defect 3 — FIXED (1e13df52d).** `GoalAccept::ffi_deserialize` no longer
hand-decodes the 17-byte goal-accept wire layout in the header. Added
`nros_cpp_action_goal_accept_decode` beside its producer in `action.rs`; the
header forwards to it. One owner for the layout.

**Defect 1 (bounded env-spin) — REMAINING.** The `NROS_ENTRY_SPIN_MS`-bounded
loop (`main.hpp::component_spin_loop` + the `src/lib.rs` copies) is a separate
surface from the duration spin above. Consolidating it entangles two things that
do not live cleanly Rust-side: the per-tick platform `yield` (`k_yield()` on
Zephyr, via `entry_tick_yield`) and the `nros::ok()` shutdown check
(`Node::global_initialized()`, a C++ static the Rust `main!` path does not
share). A `nros_cpp_spin_bounded` CFFI needs yield + shutdown hooks first. Left
for a follow-up; no confirmed live-path bug (the `bound_ms==0` shutdown-check
divergence needs its reachability confirmed before it is worth a targeted fix).

**Defect 2 (init ladder) — REMAINING.** `init()` resolves the baked
`NROS_ENTRY_LOCATOR`/`NROS_ENTRY_DOMAIN_ID` rungs + the hosted default in the
header, duplicated across the 2-arg and 3-arg overloads. Moving the precedence
into `nros_cpp_init` is an ABI signature change (the `-D NROS_ENTRY_*` compile
macros are only visible in the C++ TU, so the header must still READ them and
pass them as params — the resolver then applies precedence). A DRY refactor with
no active bug; deferred.
