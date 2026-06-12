---
id: 45
title: FreeRTOS Entry-pkg cargo build fails — Component `staticlib` crate-type needs a no_std `#[panic_handler]` that collides with the Entry bin's
status: resolved
type: bug
area: freertos
related: [issue-0041, phase-212, issue-0046]
---

## Resolution (2026-06-13)

Applied the three-gap fix designed + validated below:

1. **Component staticlib (candidate 1).** The 6 pure-cargo FreeRTOS Component
   examples (`examples/qemu-arm-freertos/rust/{talker,listener,service-server,
   service-client,action-server,action-client}`) now declare `crate-type =
   ["rlib"]` only — consumed via pure-cargo (the Entry pkg), not cmake, so the
   embedded-link `staticlib` was over-specified; dropping it removes the
   no_std-`staticlib`-needs-`#[panic_handler]` requirement on the rlib.
2. **Board-owned panic handler (candidate 2, in the board crate).**
   `nros-board-mps2-an385-freertos/src/lib.rs` now carries
   `#[cfg(target_os = "none")] use panic_semihosting as _;`. The board crate is
   no_std cortex-m-only and already deps `panic-semihosting`, so this brings the
   `panic_impl` lang item to every Entry bin that links the board rlib — the
   Entry pkg needs no handler (the `nros::main!()` macro never re-injected the old
   board-descriptor handler), and the host build is unaffected by the
   `target_os = "none"` gate.
3. **Linker-script drift.** `talker_entry/.cargo/config.toml` now links
   `-Tmps2_an385.ld` + `--nmagic` (the board build.rs's own script, emitted to
   OUT_DIR) instead of the generic cortex-m-rt `-Tlink.x` — matching the working
   freertos bins.

**Verified:** `freertos_rs_talker_entry` (`thumbv7m-none-eabi`) compiles, links
(no `panic_handler` / duplicate `panic_impl` error), and boots through the full
board lifecycle under QEMU (banner → LAN9118 + lwIP → MAC/IP assigned).

**Residual → #46:** the app task then hits `*** STACK OVERFLOW: nros_app ***` at
Executor creation because the firmware links both `zpico_sys` (zenoh) and
`nros_rmw_cyclonedds` (via the Component's `rmw-cffi` umbrella) despite
`rmw = "zenoh"`. That is rmw-backend-selection + stack tuning, tracked
separately as **#46**; the runtime test `freertos_board_run_executes_run_plan`
stays `#[ignore]`d on #46, no longer on this issue.

---

_Original report below._

## Symptom

Building the Phase 212 FreeRTOS Entry-pkg fixture
`examples/qemu-arm-freertos/rust/talker_entry/` for `thumbv7m-none-eabi`
(`cargo build --bin freertos_rs_talker_entry`) fails compiling the sibling
Component pkg:

```
   Compiling freertos_rs_talker v0.1.0 (.../examples/qemu-arm-freertos/rust/talker)
error: `#[panic_handler]` function required, but not found
error: could not compile `freertos_rs_talker` (lib) due to 1 previous error
```

This blocks the Phase 212.O.1 runtime acceptance test
`packages/testing/nros-tests/tests/freertos_run_plan_runtime.rs`
(`freertos_board_run_executes_run_plan`), which stays `#[ignore]`d.

## Root cause

The Component pkg declares the Phase 212 mandated crate types:

```toml
# examples/qemu-arm-freertos/rust/talker/Cargo.toml
[lib]
crate-type = ["rlib", "staticlib"]
```

`cargo build -v` shows that even as a *dependency* of the Entry pkg, cargo
invokes rustc with **both** crate types in one pass:

```
rustc --crate-name freertos_rs_talker ... --crate-type rlib --crate-type staticlib ... --target thumbv7m-none-eabi
```

A no_std `staticlib` is a final link artifact, so rustc requires a
`#[panic_handler]` for it. `freertos_rs_talker` only defines one under
`#[cfg(any(target_os = "linux", target_os = "macos"))]` (the host shim), so the
embedded `thumbv7m` build has none → hard error.

The naive fix — add a `#[panic_handler]` to the Component crate — does **not**
work: because rlib and staticlib are emitted from the *same* rustc invocation, a
handler in the crate also lands in the rlib, and the rlib is linked into the
Entry bin which already provides `panic-semihosting` →
`error: found duplicate lang item 'panic_impl'`.

So the two crate-type outputs have contradictory panic-handler requirements:

- `staticlib` (the C / cmake / Corrosion embedded path) **needs** an in-crate handler.
- `rlib` (the pure-cargo Entry-pkg path) **must not** define one — the Entry bin
  (or its `nros-board-*` shim) owns `panic-semihosting`.

Cargo has no per-consumer / per-crate-type conditional handler mechanism, so this
is a design decision, not a one-line fix.

## Candidate resolutions (Phase 212.O.1 design)

1. **Drop `staticlib` from the Component pkg in the pure-cargo path** and add it
   back only for the cmake/Corrosion path (e.g. a feature-gated `[lib]`
   crate-type, or a separate thin staticlib wrapper crate). Keeps the rlib
   handler-free.
2. **Move `panic-semihosting` out of the Entry bin** and let the Component rlib
   own the embedded panic handler (so both rlib and staticlib carry it, and the
   bin inherits it from the rlib). Requires every Entry bin to *not* declare a
   handler — a convention shift.
3. **A dedicated `nros-board-*` panic-handler crate** linked by the bin, with the
   Component staticlib built handler-less via a build mode that tolerates the
   missing handler (not currently possible for a no_std staticlib).

Option 1 is the least invasive to the established "Entry bin owns the board
lifecycle + panic" convention.

## Already landed (necessary but insufficient)

- `talker_entry/Cargo.toml` now carries `[profile.dev]`/`[profile.release]`
  `panic = "abort"` (profiles only apply from the root crate, so the Entry pkg
  must set it — the Component's own profile is inert when it is a dependency).
- `freertos_run_plan_runtime.rs` injects `NROS_PLATFORM_FREERTOS_SRC` /
  `NROS_PLATFORM_CFFI_INCLUDE` into its `cargo build` (the standalone example
  carries no `just freertos` overlay to set them).

These let the build progress to the panic-handler error above; the crate-type
panic-handler conflict is the remaining blocker.

## Design exploration + chosen approach (2026-06-12)

**Design-of-record: [RFC-0032 §3.1](../design/0032-entry-codegen-pipeline.md)**
(boot-scaffold completeness) + [RFC-0024 §6.4](../design/0024-multi-node-workspace-layout.md)
(Node-pkg crate-type). Work items tracked in
[phase-212 §212.O.1](../roadmap/phase-212-ux-cargo-native-and-file-consolidation.md).

Explored the design end-to-end and **validated the fix experimentally** (changes
made, build/boot verified, then reverted pending a scoped implementation).

**Chosen approach: board crate owns the embedded panic handler** (a variant of
candidate 2, but in the *board* crate, not the Component). The board crate
(`nros-board-mps2-an385-freertos`) is `#![no_std]`, cortex-m-only, and already
deps `panic-semihosting` unconditionally — it cannot compile on host, so a
`#[cfg(target_os = "none")] use panic_semihosting as _;` at its crate root is
safe and brings the `panic_impl` lang item to any bin that links the board rlib.
The Entry pkg stays untouched (no panic dep, no macro injection). Combined with
**candidate 1** (Component → `["rlib"]` only, staticlib is a cmake/Corrosion-path
concern), this clears both panic gaps.

**Three independent gaps were uncovered (not just one):**

1. **Component staticlib** (this issue's title) — fixed by `crate-type =
   ["rlib"]` on the 6 pure-cargo FreeRTOS Component examples. The cmake-consumed
   threadx fixtures already declare `["staticlib"]` only + build for host, so the
   crate-type is deployment-path-specific; the spec's "irreducible
   `["rlib","staticlib"]`" overspecified.
2. **Entry-bin panic handler lost in the `nros::main!()` migration** — the board
   descriptor's `crate_root_extra = "use panic_semihosting as _;"` was injected by
   the *old* `nros codegen-system` path (`generate.rs:768`); the Phase-213.C.1
   `nros::main!()` macro never consumes it, so the Entry bin had no handler.
   Fixed by the board-owned approach above (no macro change needed).
3. **Linker-script config drift** — `talker_entry/.cargo/config.toml` pins
   `-Tlink.x`, but the board descriptor's `cargo_config` specifies
   `-Tmps2_an385.ld` + `--nmagic` (and the board build.rs emits `mps2_an385.ld`
   to its `OUT_DIR`, which its own comment says the config should reference). The
   committed example config is stale.

**Validation result:** with all three applied, `freertos_rs_talker_entry` for
`thumbv7m-none-eabi` **compiles, links, and boots** through the full board
lifecycle under QEMU — banner → `Initializing LAN9118 + lwIP` → MAC/IP assigned.

**Residual (separate, deeper gap — the "O.1 runtime stabilisation"):** the app
task then hits `*** STACK OVERFLOW: nros_app ***` at Executor creation, BEFORE
the run_plan body. `app_stack_bytes` already defaults to 256 KB, so the overflow
is from the inline Executor arena — the firmware links **both** `zpico_sys`
(zenoh, the board default) AND `nros_rmw_cyclonedds` (pulled via the Component's
`nros` umbrella `rmw-cffi`), even though the deploy config says `rmw = "zenoh"`.
Resolving this is rmw-backend-selection + stack/heap tuning, NOT the panic-handler
design — track as the O.1 runtime tail (here or a sibling issue).

## Not blocked by this

Phase 212 M-F.17 (`nros plan` source-metadata α-bridge) is landed and validated
— the planner-side acceptance tests (`board_agnostic_run_plan`, `pkg_index`,
`nav2_compat`, `threadx_corrosion_bringup`) are un-`#[ignore]`d and green. Only
the O.1 FreeRTOS runtime-link test remains gated, on this issue.
