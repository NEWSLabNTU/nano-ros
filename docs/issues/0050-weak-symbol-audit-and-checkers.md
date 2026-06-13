---
id: 50
title: Audit existing weak symbols + add checkers — weak linkage is bug-prone (ordering/GC/ODR)
status: open
type: tech-debt
area: build
related: [issue-0042, phase-241]
---

## Why

Weak symbols (`__attribute__((weak))` in C, `.weak` in asm, the
`nros_app_register_backends` weak/strong "dance") are **bug-prone**: which
definition the linker keeps depends on archive order, `--gc-sections`, and
`--whole-archive`, and a weak symbol can be silently dropped or the wrong copy
chosen with no error. The failure mode is a runtime mis-behaviour, not a link
error — the worst kind (cf. the #48-class "registered into the wrong instance"
hazard, and why RFC-0042 D3 slice 4 explicitly **rejected** a weak-symbol fix for
the `nros-rmw-cffi` dedup in favour of a define-once export macro).

The codebase already uses weak symbols in ~35 C sites + the register stub. They
are **unaudited and unguarded**: nothing verifies that each weak default is
actually overridden where intended, that no two strong defs silently fight, or
that a weak symbol survives `--gc-sections`/`--whole-archive` on every platform.
A new board / example / link-flag change can quietly break one.

## Current weak-symbol surface (survey 2026-06-13, non-exhaustive)

Override-default pattern (a weak default, a board/app supplies the strong def):
- `packages/boards/nros-board-common/c/threadx_hooks.c` — `nros_board_app_stack_size`,
  `nros_board_app_priority`, `nros_board_log`, `nros_board_init_eth`,
  `nros_board_compute_rng_seed`, `app_main`.
- `packages/boards/nros-board-freertos/c/network_glue.c` — `nros_board_register_netif`,
  `nros_board_poll_netif`.
- `packages/boards/nros-board-mps2-an385-freertos/startup.c:628` — `weak, used`.
- `packages/boards/nros-board-threadx-qemu-riscv64/c/tx_initialize_low_level.S:139`
  — `.weak _tx_initialize_low_level`; `board_threadx_qemu_riscv64.c` weak default.
- `packages/zpico/zpico-sys/c/zpico/platform_aliases.c` — `_z_*_serial_*`,
  `smoltcp_init`/`smoltcp_cleanup` (transport-alias stubs).
- `packages/dds/nros-rmw-cyclonedds/src/vtable.cpp:137` —
  `nros_rmw_cyclonedds_register_app_descriptors` weak no-op.
- The `nros_app_register_backends()` weak no-op (nros-c weak stubs) overridden by
  the cmake-generated strong stub (`NanoRosLink.cmake`, `nros-build-helpers`,
  `nros-c/src/support.rs`).

Rust `#[linkage = "weak"]`: none today (the toolchain is stable; the attribute is
unstable). Keep it that way.

## Scope for the worker

1. **Audit.** Enumerate every weak symbol (C `__attribute__((weak))`, asm `.weak`,
   any Rust weak), classify each: (a) legit override-default (a strong def is
   guaranteed elsewhere), (b) optional hook (no-op is the intended fallback),
   (c) fragile / accidental. For each, record where the strong def comes from and
   on which platforms.
2. **Add checkers.** A merge-gate that, per linked artifact / per platform:
   - lists weak symbols in the final image and flags any **unexpected** weak
     symbol (an allowlist of the intended override-defaults, mirroring the
     `staticlib_duplicate_symbols.rs` allowlist approach for the dup gate);
   - asserts each **override-default** weak symbol is actually overridden by a
     strong def in the final link (not silently left as the weak no-op when a real
     impl was intended — the threadx_bringup "weak stubs → real-entry assertion
     fails" hazard already noted in `threadx_corrosion_bringup.rs`);
   - is robust to `--gc-sections` / `--whole-archive` (the conditions that flip
     weak resolution).
3. **Reduce.** Where a weak default exists only to dodge a link-order problem
   (not a genuine optional hook), prefer a define-once / explicit-registration
   structure (cf. RFC-0042 D3) over weak.

## References

- RFC-0042 / [phase-241](../roadmap/phase-241-platform-build-determinism.md) D3
  (deterministic linking) — the slice-4 design rejected weak for exactly this
  fragility and chose a define-once export macro; the same reasoning motivates
  auditing the existing weak surface.
- `packages/testing/nros-tests/tests/staticlib_duplicate_symbols.rs` — the
  duplicate-symbol gate; the weak checker can mirror its `llvm-nm` + allowlist
  shape.
