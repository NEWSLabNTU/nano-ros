# Phase 249 — one RMW registration trigger (RFC-0042 §D3 bullet 1)

Status: **Design — approved 2026-06-14** · Implements RFC-0042 §D3 bullet 1 ·
Phase-241 W13/R3 · Tracked by [issue 0062](../issues/0062-d3-completion-one-registration-path-and-link-manifest.md)
· Unblocks R2 (the weak-default + stub deletion that closes [issue 0050](../issues/0050-weak-symbol-audit-and-checkers.md) W3.1).

Single-runtime (phase-241 W1–W12) delivered D3 bullet 3 (the std/cffi dup) and W13/R1
delivered bullet 2 (the generated dispatch manifest). This phase delivers **bullet 1 —
one registration path** — on top of that foundation.

## Problem — four belt-and-suspenders triggers, none universal

Every RMW backend ultimately registers by calling `nros_rmw_<x>_register()` (→ the cffi
`REGISTRY`). Today **four** mechanisms try to make that call fire, layered because **no
single one works on every target**:

| # | Trigger | Fires on | Silent no-op / fails on |
| --- | --- | --- | --- |
| 1 | linkme `RMW_INIT_ENTRIES` distributed slice (walked by `__register_linked_rmw` / `Executor::open`) | hosted Rust (linux/macOS) | **RTOS** — linkme doesn't recognise FreeRTOS/NuttX/Zephyr/ESP-IDF section names → walker finds 0 entries (Phase 155.B.4) |
| 2 | `.init_array` ctor (`nros-c`/`nros-cpp` `rmw_backend`, W11 synth) | hosted; bare-metal **iff** board startup walks `.init_array` | bare-metal whose startup does not walk it |
| 3 | explicit `nros_app_register_backends()` — `nros_support_init` / `nros_cpp_init` call it **unconditionally** | **everywhere** (it is a plain call) | only if no **strong** def exists (the weak no-op is a no-op) |
| 4 | board `entry.rs` explicit `__register_linked_rmw()` | bare-metal Rust (board cooperates) | — |

The weak/strong dance: `weak_register_backends.c` ships a weak no-op `nros_app_register_backends`;
cmake `nano_ros_link_rmw()` generates a **strong** override per linked backend (the only
real registration on RTOS C/C++). This is the #48-class hazard (issue 0050 W3.1): a
missing strong def silently degrades to the no-op.

## Decision — the one trigger is the explicit generated call

This is the faithful implementation of **RFC-0042 §D3 bullet 1**, which already specifies
it: *"Codegen emits an explicit backend-register table for the binary (the set of
`nros_rmw_<x>_register()` to call), used on all platforms — hosted included. The
linkme-vs-weak split is removed … the distributed-slice may remain an implementation
detail of the generator's hosted path but is no longer a second contract. Bare-metal and
hosted register identically."* (Issue 0062's earlier "fold into the `.init_array` ctor"
framing was a deviation — the ctor is not universal — now corrected back to the RFC.)

Mechanisms 1, 2, 4 each fail on some target; only **the explicit call (3) is universal**
(no linker-section / ctor walking to skip per-platform). W13/R1 already made the SSoT
(`resolve_rmw()` / `RmwDispatch`) know `backend → register fn` — the "register table" the
RFC names. So:

> **Registration is exactly one explicit `nros_rmw_<backend>_register()` call per binary,
> generated from the R1 dispatch manifest, identical on every platform. The `.init_array`
> ctors and the weak default are retired; linkme stops being a registration contract.**

**On linkme.** The RFC permits the distributed slice to *remain* as a hosted-only
implementation detail. Phase-249 instead uses the **uniform explicit call on hosted too**
(not a hosted linkme branch): one code path, no per-platform impl split, and — decisively
— it is the uniform explicit call that lets the weak `nros_app_register_backends` default
die (P4/R2). A hosted-only linkme impl would keep the weak/strong split alive on hosted.
The `RmwInitEntry` *type* + an empty slice may stay if an out-of-tree consumer needs them;
only the registration *role* is retired.

Per language:

- **C / C++.** `nros_support_init` / `nros_cpp_init` keep their single
  `nros_app_register_backends()` call, but that symbol becomes a **generated STRONG def**
  emitted once from the manifest (the backend's `nros_rmw_<x>_register`), replacing both
  the weak no-op and the ad-hoc per-target cmake stub. One def, always strong, on every
  platform.
- **Rust.** The `nros::main!()` macro and the board `entry.rs` emit one explicit
  `nros_rmw_<backend>_register()` (the backend the board/deploy selected — known via the
  manifest / the board's `rmw-<x>` feature), replacing the linkme-slice walk in
  `__register_linked_rmw`.

Net: a missing registration is a **link error** (undefined `nros_rmw_<x>_register`), never
a silent `NoBackend`. The phase-247 weak-symbol **image gate** asserts the register symbol
resolves strong — it guards the retirements.

### Retired

- The linkme `RMW_INIT_ENTRIES` distributed slice + `nros_rmw_cffi_walk_init_section` +
  the `NROS_RMW_REGISTER_BACKEND` C macro (its registration role; the section may stay as
  an *empty* stub only if some out-of-tree consumer still needs the type).
- The `.init_array` ctors in `nros-c`/`nros-cpp` `rmw_backend` and the W11 synth.
- The weak no-op `nros_app_register_backends` in `weak_register_backends.c` (keep the
  sibling `nros_platform_log_{write,flush}` weak fallbacks — separate concern).
- The ad-hoc cmake stub in `nano_ros_link_rmw()` (replaced by the generated strong def).

## Work items — phased, each gated by per-platform e2e

Order minimises blast radius: migrate each path to the explicit call **before** deleting
its old trigger, so every intermediate state still registers (belt kept until suspenders
proven).

- **P1 — Rust path to explicit call.** `nros::main!()` + board `entry.rs` emit one
  explicit `nros_rmw_<backend>_register()` from the selected backend; keep linkme as the
  fallback for now. **Gate:** native Rust + FreeRTOS + ThreadX-rv64 Rust e2e register +
  run (the linkme-blind RTOS path now has the explicit call; hosted unchanged).
- **P2 — C/C++ generated strong def.** Emit `nros_app_register_backends` as a generated
  STRONG def from the manifest (CLI codegen or a generated TU the cmake compiles),
  replacing the per-target `nano_ros_link_rmw` stub. **Gate:** native C/C++ + FreeRTOS +
  ThreadX + NuttX C/C++ e2e; the image gate stays green.
- **P3 — drop the `.init_array` ctors.** With P1+P2 guaranteeing the explicit call, the
  ctors are redundant. Remove them from `nros-c`/`nros-cpp` `rmw_backend` + the W11 synth
  anchor (keep the `FORCE_LINK` that pulls the backend closure, now referenced by the
  explicit call). **Gate:** hosted + workspace (mixed) + a bare-metal cell.
- **P4 — delete linkme slice + the weak no-op (closes R2 / issue 0050 W3.1).** Remove the
  distributed slice + the weak `nros_app_register_backends`; a missing backend is now a
  link error. **Gate:** full per-cell e2e (the W7 matrix) + `just check` + the weak-symbol
  image gate; `examples/workspaces/mixed` + a pure-Rust + a C/C++ + an RTOS cell each
  register + run.

## Acceptance

- Exactly one registration trigger across C/C++ + pure-Rust + embedded: the explicit
  `nros_rmw_<backend>_register()` call, sourced from the R1 manifest. `git grep` shows no
  linkme `RMW_INIT_ENTRIES` registration path and no `.init_array` rmw ctor remaining.
- The weak `nros_app_register_backends` default + the cmake stub are gone; a missing
  registration fails the link (image gate green) — closes issue 0050 W3.1 / W13 R2.
- The full per-cell e2e matrix (W7) is green: every platform × language registers the
  backend and runs (no `NoBackend`).

## Risks

- **Silent `NoBackend` on a missed platform.** Mitigation: migrate-before-delete (P1–P3
  keep the old trigger), the phase-247 image gate, per-phase e2e gates.
- **Board-entry diversity.** Each board's `entry.rs` registers slightly differently
  (FreeRTOS/ThreadX/STM32/FVP — see the `__register_linked_rmw` call sites); P1 must cover
  each board's selected-backend knowledge (the board's `rmw-<x>` feature is the SSoT).
- **Out-of-tree linkme consumers.** If any exist, keep the `RmwInitEntry` type as an empty
  stub; only the *registration* role is retired.

## References

- [phase-241 W13](phase-241-d3-single-runtime.md) — R1 (done) / R2 (this unblocks) / R3 (this).
- [issue 0062](../issues/0062-d3-completion-one-registration-path-and-link-manifest.md) — the tracker.
- [issue 0050](../issues/0050-weak-symbol-audit-and-checkers.md) W3.1 — the weak-default deletion P4 closes.
- [phase-247 weak-symbol determinism](phase-247-weak-symbol-determinism.md) — the image gate that guards the retirements.
- RFC-0042 §D3 bullet 1 — the goal.
