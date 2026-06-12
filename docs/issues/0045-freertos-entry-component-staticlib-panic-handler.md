---
id: 45
title: FreeRTOS Entry-pkg cargo build fails — Component `staticlib` crate-type needs a no_std `#[panic_handler]` that collides with the Entry bin's
status: open
type: bug
area: freertos
related: [issue-0041, phase-212]
---

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

## Not blocked by this

Phase 212 M-F.17 (`nros plan` source-metadata α-bridge) is landed and validated
— the planner-side acceptance tests (`board_agnostic_run_plan`, `pkg_index`,
`nav2_compat`, `threadx_corrosion_bringup`) are un-`#[ignore]`d and green. Only
the O.1 FreeRTOS runtime-link test remains gated, on this issue.
