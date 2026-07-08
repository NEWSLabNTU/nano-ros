---
id: 159
title: "NuttX fixture build can report success with no kernel ELF — `cmake --build <entry>_build` returns 0 on some link failures"
status: resolved
type: tech-debt
area: build
related: [phase-281, issue-0149]
---

## RESOLVED (2026-07-08) — backstop restored after clobber-reversion + in-command ELF verify landed

Checking this issue found the backstop MISSING at HEAD: `7ad8cc894` (the hash
in "Mitigation landed" had a typo) had been reverted by the stale-tree clobber
commit `f344492e4` and never restored — `workspace-fixtures-build.sh` was back
to the lenient "built target" branch. Restored by cherry-pick, together with
the OTHER still-lost clobber victim `791677222` (rust_nuttx_entry_e2e String
listener prefix + c_nuttx_entry_e2e in the qemu-nuttx nextest group) — a
full-window audit (HEAD blob == post-clobber blob per file) shows these two
files were the last remaining losses. Both entry e2e tests pass after
restoration (rust 8.8 s, c 10.2 s).

The "Direction" fix also landed: `nros_nuttx_build_example`'s custom command
now verifies the kernel ELF exists as its final step (`test -f` after the
cargo cross-link in `nros-nuttx.cmake`), so `cmake --build` itself fails loud
regardless of which sub-step swallowed the failure — two independent layers
now cover the failure mode. The original exit-0 edge did not reproduce: a
forced compile failure through the same custom command propagated exit 101
out of `cmake --build` (ninja generator). Root remains unidentified but is now
double-defended; any recurrence fails loud at both layers and leaves fresh
evidence.

Fallout fixed en route (both latent breaks exposed by the fresh rebuild):
- `cmake/templates/zephyr_entry_main_c_typed.cpp.in` had been corrupted by a
  stray clang-format pass in `701ae4b6a` — reflow split the `@NROS_ENTRY_PKG_SYM@`
  configure_file tokens (`@NROS_ENTRY_PKG_SYM @_create`), so every freshly
  configured typed-C zephyr entry TU failed with "stray '@'". Repaired; a
  `.clang-format-ignore` for `cmake/templates/*` prevents recurrence.
- `component.h`'s local `nros_cpp_qos_t` mirror (the `#ifndef NROS_CPP_FFI_H`
  branch) was missing the phase-282 `tx_express` field the real
  `nros_cpp_ffi.h` struct has — mirror-only TUs had a SHORTER struct than the
  FFI consumer (by-value ABI mismatch, the #131 stale-mirror class); #157's
  `q.tx_express = 0` init made it loud on the NuttX C lane. Field appended.

Verified: zephyr cyclonedds fixtures rebuild fresh + phase_118 8/8; NuttX C
talker example builds through the verifying custom command; both NuttX entry
e2e green.

## Summary

A NuttX workspace/example fixture can finish its build with exit 0 yet produce
**no bootable kernel ELF**. Surfaced during phase-281 W3-nuttx: a link failure
was only detected by explicitly checking for the artifact, not by the build's
exit code.

## Mitigation landed

`scripts/build/workspace-fixtures-build.sh` now **fails loud** for NuttX when the
build produces no `<entry>` kernel ELF (empty artifact-find → `return 2` instead
of the old lenient "built target" branch that still stamped a non-existent
fixture) — commit `7ad8cc884`. This is a robust backstop: any missing-ELF case
now fails the build, regardless of root.

## Untracked root

The backstop does not explain WHY `cmake --build --target <entry>_build` can
return 0 with no artifact. The final flat-build link goes through cargo/rustc
(`nros-nuttx-ffi`'s build.rs emits `cargo:rustc-link-arg` + the cc-rs archives;
the cargo binary IS the kernel image), and cargo/rustc + cc-rs `.compile()`
propagate their own failures — so most link errors *do* surface a non-zero exit.
The observed exit-0 edge is therefore likely one of:

- a cmake **up-to-date skip** (a stale/absent `_output_binary` whose `DEPENDS`
  didn't change, so the custom command isn't re-run), or
- a sub-step inside the custom command whose failure isn't propagated
  (`${_provision_cmd}` prefix, or an env/cc step).

## Direction (optional — the backstop covers the failure)

If it recurs, instrument the `nros_nuttx_build_example` custom command
(`packages/core/nros-c/cmake/nros-nuttx.cmake`) to `BYPRODUCTS` the ELF + verify
it inside the command (fail the command, not just the outer script), so
`cmake --build` itself is honest. Low priority — the workspace-fixtures backstop
already prevents a broken fixture from being stamped as built.
