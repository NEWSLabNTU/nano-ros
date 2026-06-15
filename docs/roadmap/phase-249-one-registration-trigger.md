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
- **Rust.** The **board boot owns the register call** — the board (selected via the
  Entry pkg's `deploy`) calls its linked `nros_rmw_<x>::register()`, gated on the board's
  own `rmw-<x>` feature, **on every OS** (bare-metal = P1; hosted = P3.5 below — the
  earlier impl left the hosted emit `#[cfg(target_os="none")]`-gated, deferring hosted to
  linkme). Not the `nros::main!()` macro and not user code: registration is framework-side
  in the board crate. **User code stays the ROS composable-node shape with ZERO
  registration boilerplate** — no `#[used]` force-link statics, no `linkme-register`
  feature, no `register()` call (design principle, user 2026-06-14). The Pattern-2
  `init_with_launch_auto` native examples (which carry the force-link block) migrate to the
  declarative Node + `nros::main!()` Entry shape, which *removes* that boilerplate.

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

- **P1 — Rust path to explicit call. DONE (2026-06-14).** Most of P1 was already
  delivered by **phase-248 C5a**: every deploy board's boot path calls its linked
  `nros_rmw_<x>::register()` (gated on the board's `rmw-<x>` feature) — the explicit
  Rust trigger. The board-agnostic `nros` crate *cannot* register (no backend dep), so
  P1's residual was removing the dead `nros::__register_linked_rmw()` no-op (a Phase-248
  C5c stub kept only so the `main!` call sites compiled) + its three emits (`main!`
  macro ×2, the Zephyr `*_component_main!` macro). The linkme **walk** (`Executor::open`)
  stays as the hosted/cyclonedds fallback (retired in P4). **Gate:** native Rust +
  ThreadX-rv64 Rust fixtures build clean (same `main!`+board path). FreeRTOS Rust is
  blocked by a *pre-existing* phase-248 residual (the fixture build passes `--features
  rmw-zenoh` to examples that relocated it to the board → cargo feature-resolution error
  *before* the macro) — not a P1 regression; tracked under phase-248's deferred FreeRTOS
  smoke.
- **P2 — C/C++ generated strong def.** Emit `nros_app_register_backends` as a generated
  STRONG def from the manifest, universally + manifest-driven.

  **Landscape audit (2026-06-14) — the strong def is INCONSISTENT across platforms:**
  | platform | strong `nros_app_register_backends`? | how C/C++ registers |
  | --- | --- | --- |
  | posix | ✅ auto | `nano-ros-posix.cmake:93` → `nano_ros_link_rmw(target)` |
  | threadx | ✅ per-example | each `examples/*/threadx*/…/CMakeLists.txt:47` calls `nano_ros_link_rmw` |
  | freertos | ❌ weak no-op | relies on backend `.init_array` ctors |
  | esp_idf | ❌ weak no-op | relies on `.init_array` ctors |
  | nuttx | ❌ weak no-op | Rust FFI crate drags the staticlib; C/C++ would fall back |
  | zephyr | ❌ (platform link empty) | explicit via the typed carrier / `nros_cpp_init` (ctors `#ifndef __ZEPHYR__`) |
  | bare-metal | ❌ weak no-op | **the canonical gap — `.init_array` unreliable** |

  **P2 work:** (a) **manifest-drive** `nano_ros_link_rmw` — source the `nros_rmw_<x>_register`
  set + the cyclonedds `NROS_RMW_NEEDS_CXX_LINKER` / `EXTRA_LINK_LIBS` from the R1
  `nros_rmw_dispatch` (SSoT) instead of ad-hoc cmake; (b) make the strong-def generation
  **universal** — every C/C++ app target gets it (move the auto-call into the shared
  `nros_platform_link_app` / entry path, not per-platform/per-example), so no C/C++ binary
  depends on `.init_array`. **Gate:** native C/C++ + ThreadX C/C++ e2e (have harnesses);
  build-check freertos/nuttx/esp/zephyr C/C++ (runtime smoke deferred with phase-248's
  embedded-harness residual); image gate green. **(a) is the contained, validatable slice
  landed first; (b) the universal rollout rides the embedded e2e harness.**

  **P2a — DONE (2026-06-14).** `nano_ros_link_rmw` sources `NROS_RMW_NEEDS_CXX_LINKER`
  from the R1 `nros_rmw_dispatch` manifest (commit 8fc4e8695).
  **P2b — DONE (2026-06-15).** Moved the strong-def auto-call into the shared
  `nros_platform_link_app` for **threadx, freertos, esp_idf, nuttx** (posix already had
  it; baremetal has no C/C++ targets + omits the helper; zephyr's link_app is empty by
  design). `nano_ros_link_rmw` is idempotent (accumulates `_NANO_ROS_LINKED_RMWS`,
  dedups), so the universal call coexists with the 31 threadx per-example explicit calls
  with no double-define. **Validated:** the threadx-linux **C** zenoh set (6 examples)
  builds + links clean — exit 0, zero `multiple definition`, the generated
  `nros_app_register_backends.c` stub compiled into each. native (posix) path unchanged.
  freertos/nuttx/esp are build-check tier (additive/guarded, same proven pattern; runtime
  on their CI). *Optional cleanup (not blocking): drop the 31 now-redundant threadx
  per-example `nano_ros_link_rmw` calls.*
- **P3 — drop the `.init_array` ctors. DONE (2026-06-15).** Removed
  `AUTO_REGISTER_CTOR` from `nros-c`/`nros-cpp` `rmw_backend.rs` (kept `FORCE_LINK` +
  the `pub auto_register` re-export — they keep the backend closure incl. the
  `nros_rmw_<x>_register` C export, which the P2b strong stub now calls). In the W11
  synth (`NanoRosRuntimeCrate.cmake`) the `_KEEP_BACKEND_CTOR` `.init_array` ctor became
  a plain `#[used]` force-link anchor (`_KEEP_BACKEND`) — and is anyway redundant there,
  since the synth's `_KEEP_SURFACE` (`nros_cpp::FORCE_LINK_ANCHOR`) already pulls the
  backend closure. **Validated:** native C + C++ build/link clean (the stub resolves
  `nros_rmw_zenoh_register` via FORCE_LINK with the ctor gone — the hosted gate);
  threadx-linux C set links clean (embedded FORCE_LINK holds). workspace-mixed synth path
  is reasoned-safe (redundant harmless anchor; the FORCE_LINK chain it rides was proven by
  the green native C++ link) — full runtime is build-tier on `just build-test-fixtures` / CI.
- **P3.5 — hosted-Rust explicit register (board boot). NEW — the P4 precondition.**
  P1 wired the board-boot `nros_rmw_<x>::register()` only for `target_os="none"`; the
  hosted emit was *removed* (main_macro.rs:859 / `nros::main!`) and hosted Rust still
  registers via the linkme walk. So linkme cannot be deleted yet. Fix (design (a),
  confirmed 2026-06-14): the board's boot calls its linked `nros_rmw_<x>::register()` on
  **hosted too** (un-gate from `target_os="none"`), gated on the board's `rmw-<x>` feature
  — `NativeBoard`/`PosixBoard::run`, mirroring the bare-metal path. The `#[used]`
  force-link static folds into the call. **User code stays the composable-node shape with
  zero registration boilerplate**: migrate the Pattern-2 `init_with_launch_auto` native
  examples (talker/listener/service/action) to the declarative Node + `nros::main!()`
  Entry shape, dropping their `#[used] __FORCE_LINK_*` block + the `linkme-register`
  feature. **Gate:** native Rust pub/sub + service + action e2e register + run with linkme
  still present (belt) but the board call doing the work (suspenders); `nros::main!` hosted
  binary registers without the linkme walk. Then P4 can delete linkme.

  **P3.5a — board enabler DONE (2026-06-15).** `NativeBoard::{run,run_tiers}` call a
  feature-gated `register_backend()` (the linked `nros_rmw_<x>::register()`) before
  delegating to `PosixBoard` — on every OS, the same call bare-metal P1 uses without
  linkme. Validated: board + entry-poc (declarative native) build clean, entry-poc runs
  identically (no regression; `Executor::open` reaches a backend → `ConnectionFailed` not
  `NoBackend`). **P4 note (found 2026-06-15):** `linkme-register` is default-on and pulled
  via *multiple* dep paths — P4 must drop the feature everywhere + the macro invocations,
  not one dep.

  **P3.5b — example migration: WITHDRAWN (conflicts with phase-244 D7, 2026-06-15).**
  The board pieces stand: `NativeBoard` extended to own zenoh/xrce/cyclonedds + the P3.5a
  board-boot register — both harmless-additive (pushed). The *example migration* (the 6
  native Pattern-2 examples → declarative Node + `nros::main!()`, board-owned register, no
  force-link) was built + **runtime-validated** (talker→listener `message_callbacks=5`
  over zenoh, board-owned registration) but **discarded**: phase-244 **D7** (`833979e59`)
  landed a contrary, empirically-verified decision — native rust keeps **Shape B**
  (Pattern-2 + the `#[used] __FORCE_LINK_*` ladder; D7's commit verified that *removing*
  the ladder breaks registration because linkme needs it). So native rust deliberately
  stays linkme-based, and the declarative migration contradicts it. The migration files
  (`<name>_pkg` dirs) are kept untracked/recoverable should the **P4b ↔ D7** fork (below)
  resolve toward the board-owned direction. *(safety-e2e / param-services / link-tls /
  zero-copy → a config-driven dimension regardless: [phase-250](phase-250-safety-params-feature-dimension.md).)*
- **P4a — delete the C/C++ weak `nros_app_register_backends`. DONE (2026-06-15).**
  The cmake `nano_ros_link_rmw` strong stub (P2b) is now the sole def; a missing one is a
  **link error**, not a silent no-op (the #48-class hazard). C/C++-only — independent of
  native-Rust linkme (which D7 keeps), so it lands cleanly. **Closes
  [issue 0050](../issues/0050-weak-symbol-audit-and-checkers.md) W3.1 / R2.** Validated:
  `just native build-c` + `build-cpp` link clean (no undefined symbol, no multiple-def);
  source + image + rust weak gates green.
- **P4b — consolidate registration onto the `.init_array` ctor (delete linkme).
  REDEFINED 2026-06-15 (decision: RFC-0042 §D3.3).** *(Was "delete linkme → explicit
  table"; the explicit table is dropped — the ctor is the trigger.)* The three
  self-register mechanisms collapse to **two**: hosted (Rust + C/C++) = the backend's
  `.init_array` ctor (loader fires it before `main`; the D7 Shape-B `#[used]
  __FORCE_LINK_*` anchor keeps it linked — **no app-source `register()` call, D7
  honoured**); embedded = the explicit board call (P1). **linkme is deleted.** Rationale
  (linkme vs ctor UX + maintainability) + the full design → RFC-0042 §D3.3. Staged:
  - **P4b.1 — Rust macro → ctor.** `nros_rmw_register_backend!` (`nros-rmw-cffi/src/section.rs`)
    emits a `#[used] link_section=".init_array"` (+ macos `__DATA,__mod_init_func`) ctor,
    gated `not(target_os="none")`; delete the `RMW_INIT_ENTRIES` slice + stub + the 12-OS
    allow-list + `nros_rmw_cffi_walk_init_section` + its 3 call sites (`Executor::open` /
    `open_multi` / `nros::init`). Model: the existing `nros-rmw-zenoh-staticlib` ctor.
    **Gate:** native Rust pub/sub e2e registers (ctor fires) + a bare-metal cell (board
    explicit unaffected).
  - **P4b.2 — C macro → ctor.** `NROS_RMW_REGISTER_BACKEND` (`rmw_vtable.h`) →
    `__attribute__((constructor))` (model: cyclonedds `vtable.cpp:198`). **Gate:** native
    C/C++ e2e.
  - **P4b.3 — drop the dep + feature.** Remove the `linkme` dep + the `linkme-register`
    feature from `nros-rmw-cffi` + the backend crates + passthroughs; delete the now-
    redundant `*-staticlib` ctors (the macro provides it). **Gate:** `just check` + the
    cross-platform fixture builds.
  - **P4b.4 — docs:** RFC-0042 §D3.3 (done), this bullet, [phase-241](phase-241-d3-single-runtime.md)
    reconcile, [issue 0062](../issues/0062-d3-completion-one-registration-path-and-link-manifest.md) R3.
  **Keep throughout:** the `#[used] __FORCE_LINK_*` anchors, the embedded explicit calls,
  the cmake `nano_ros_link_rmw` stub (P2b/P4a).

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
