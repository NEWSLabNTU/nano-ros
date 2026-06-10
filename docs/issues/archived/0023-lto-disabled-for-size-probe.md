---
id: 23
title: LTO disabled workspace-wide so the opaque-size probe can read symbol byte sizes
status: resolved
type: tech-debt
area: build
related: [phase-87, phase-118, phase-234]
resolved_in: Phase 234
---

`[profile.release]` pinned `lto = "off"` only so `nros-sizes-build::extract_sizes`
could read `__NROS_SIZE_*` static **byte sizes** via the `object` crate — which can't
parse the LLVM-bitcode rlib members LTO emits (every size came back `0`).

**Resolution (Phase 234).** The probe already had a bitcode-agnostic second path:
`extract_sizes_via_llvm_nm` runs rustc's bundled `llvm-nm --demangle` and parses the
Phase 77.25 markers `__nros_size_<NAME>::<N>` (size baked into the v0-mangled symbol
*name*, so LTO-independent). Flipped `[profile.release] lto = "fat"`.
`[profile.nros-fast-release]` keeps its explicit `lto = "off"` (inherits release, so
the override is required to keep fast-iteration builds LTO-free).

**Validation.**
- Probe sizes **byte-identical** `lto=off` vs `lto=fat`, read by host `llvm-nm`, on
  host (`528/560/79560`), `thumbv7m-none-eabi` (`520/552/79208`), and
  `armv7a-nuttx-eabihf` (all 17 markers — the filesystem-fallback target, most at
  risk). `nros-c` / `nros-cpp` link clean at `release`.
- `just rust-rtos-link-check` green under fat LTO (freertos + nuttx + threadx-linux).
- Size win on `logging-smoke-mps2-baremetal`: `.text` 7488 → 5636 (−24.7%).
- Regression guard: `packages/core/nros-sizes-build/tests/bitcode_probe.rs`.

**Scope.** The flip targets the *root* workspace `[profile.release]`; standalone
firmware example workspaces adopt LTO per-workspace (nuttx already did, out of boot
necessity). Sizes are layout-determined (`size_of::<T>()` ← target triple), so LTO
never changes them.

See `docs/roadmap/archived/phase-234-enable-lto.md` for full work-item evidence.
