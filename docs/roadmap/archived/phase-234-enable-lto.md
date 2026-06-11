# Phase 234 — Enable LTO (the size probe already reads bitcode)

**Goal.** Remove the workspace-wide `lto = "off"` pins so embedded release builds
get the link-time-optimization size/perf win. The constraint that forced them — the
opaque-size probe could only read native-object symbol byte sizes — is already
lifted: `nros-sizes-build` has a bitcode-aware `llvm-nm` reader, validated to
recover every size under `lto = "fat"`. This phase flips the profiles and proves the
probe consumers (`nros-c`, `nros-cpp`) get identical sizes across the target matrix.

**Status.** Done (2026-06). Profiles flipped; probe validated byte-identical on host +
`thumbv7m-none-eabi` + `armv7a-nuttx-eabihf` (the filesystem-fallback target);
regression test landed; size win measured (−24.7% `.text` on a baremetal smoke
firmware); `just rust-rtos-link-check` passes under fat LTO (freertos + nuttx +
threadx-linux). Resolved issue
[0023](../issues/archived/0023-lto-disabled-for-size-probe.md).

**Scope note.** The flip targets the *root workspace* `[profile.release]`. Nearly all
embedded firmware (freertos / nuttx / threadx / zephyr examples) are **standalone
workspaces** that set their own profiles, so the flip does not reach their image
links — those adopt LTO per-workspace (nuttx already builds at `release`+LTO out of
necessity: `nros-fast-release`'s `lto=off` triggers a cross-CGU miscompile that bricks
boot). The flip's real blast radius is the root-workspace libs + the FFI probe
consumers (`nros-c` / `nros-cpp`), both validated to link clean at `release`.

**Priority.** P2 — pure size/perf win on space-constrained MCUs; no correctness gate.

**Depends on.** Issue 0023 (root cause + validation), `nros-sizes-build`
(`extract_sizes` + `extract_sizes_via_llvm_nm`), the `llvm-tools` toolchain
component (already in `rust-toolchain.toml`), Phase 87/118 (probe design).

## Overview

`[profile.release]` and `[profile.nros-fast-release]` pin `lto = "off"`
(`Cargo.toml:624,637`) solely so `extract_sizes` can read `__NROS_SIZE_*` static
**byte sizes** via the `object` crate, which can't parse the LLVM-bitcode rlib
members LTO emits. But the probe already has a second path:
`extract_sizes_via_llvm_nm` runs rustc's bundled `llvm-nm --demangle` and parses the
Phase 77.25 markers `__nros_size_<NAME>::<N>` (size baked into the symbol *name*).
Validation (issue 0023): a `CARGO_PROFILE_RELEASE_LTO=fat` build of `nros` produces a
bitcode rlib whose markers `llvm-nm` reads intact (`SESSION_SIZE::<528>`,
`PUBLISHER_SIZE::<560>`, `EXECUTOR_SIZE::<79560>`) — fat-LTO does not prune them, and
the `saw_bitcode` gate fires (member magic `BC\xC0\xDE`). So the pins are stale.

Sizes are layout-determined (`size_of::<T>()` depends on the target *triple*, not on
LTO), so the recovered values must be **identical** with LTO on — that is the
acceptance bar.

## Architecture

```
nros (lto=fat) ─► bitcode rlib ─► extract_sizes
                                    ├─ object/ELF byte-size path → 0 (bitcode) → out empty
                                    └─ llvm-nm --demangle path → __nros_size_NAME::<N> → sizes
                                         (host llvm-nm reads cross-target bitcode names;
                                          N baked per-target at monomorphisation)
                                  ▼
                      nros-c / nros-cpp build.rs → *_OPAQUE_U64S header macros
```

## Work Items

### 234.1 — Baseline capture  ✅
Captured the `lto=off` probe outputs as the golden reference. Host: `SESSION=528`,
`PUBLISHER=560`, `EXECUTOR=79560`; `thumbv7m-none-eabi`: `SESSION=520`,
`PUBLISHER=552`, `EXECUTOR=79208` (32-bit layout). Both probe paths (`object`
byte-size and `llvm-nm` name-based) agree under `lto=off`.
- **Files:** none (capture only).

### 234.2 — Flip the profiles  ✅
`[profile.release] lto = "fat"`; stale `lto = "off"` comment replaced with the
Phase-234 rationale. **`[profile.nros-fast-release]` keeps its explicit
`lto = "off"`** — it `inherits = "release"` (now fat), so the override is *required*
to keep fast-iteration builds LTO-free; dropping it would silently re-enable fat LTO
there. Per-crate `lto` opt-ins in smoke / logging-smoke fixtures untouched.
- **Files:** `Cargo.toml` (`[profile.release]`).

### 234.3 — Validate sizes across the target matrix  ✅ (host + thumbv7m + nuttx)
Rebuild the probe consumers under LTO and assert the recovered sizes **equal the
234.1 baseline**:
- **host** ✅ — `CARGO_PROFILE_RELEASE_LTO=fat` build of `nros` → bitcode rlib;
  `llvm-nm` fallback recovers `SESSION=528`, `PUBLISHER=560`, `EXECUTOR=79560` —
  byte-identical to baseline. `nros-cpp` end-to-end under fat LTO emits
  `CPP_EXECUTOR_OPAQUE_U64S = 9946` (matches `79560/8 = 9945` + `CPP_CONTEXT_OVERHEAD`
  rounding). No `0` / no `*_OPAQUE_U64S = 1` placeholder.
- **`thumbv7m-none-eabi`** ✅ — `lto=off` and `lto=fat` both recover `520/552/79208`
  (32-bit layout), confirming host `llvm-nm` reads cross-target bitcode names and the
  size `N` is baked per-target at monomorphisation.
- **`armv7a-nuttx-eabihf`** ✅ — built `nuttx-rs-service-server` (custom target,
  `build-std`, patched libc) `lto=off` and `lto=fat`; both compile clean. Host
  `llvm-nm` reads the fat-LTO `nros` rlib's markers **byte-identical** to the
  `lto=off` rlib — all 17 (`EXECUTOR=79328`, `PUBLISHER=552`, `SESSION=520`, …)
  match. This is exactly the filesystem-fallback path's extract step (it reads the
  outer rlib via `extract_sizes`), so that path is proven on the most-at-risk target.
  The example builds a `staticlib` (`.a`) linked into the kernel image by the nuttx
  `make`/gcc build (not `rust-lld`), so the final-ELF link is out of scope here.
Any divergence (esp. a `0` or a placeholder `*_OPAQUE_U64S = 1`) is a fail.
- **Files:** none (verification); fixes land in 234.4 if a gap surfaces.

### 234.4 — Harden the fallback  ✅
234.3 surfaced no Linux gap, but two latent robustness bugs were fixed anyway:
- **Mach-O bitcode gate** — `saw_bitcode` only matched raw `BC\xC0\xDE`; added the
  Darwin bitcode-wrapper magic `0x0B17C0DE` (LE `\xDE\xC0\x17\x0B`) so macOS hosts
  (a documented POSIX target) gate correctly instead of silently probing `0`.
- **`lib.rmeta/` skip** — GNU `ar` terminates member names with `/`, so the
  `== b"lib.rmeta"` skip never fired; strip a trailing slash before matching.

Not done (deliberately): making the `llvm-nm` path **primary**. That was gated on the
`out.is_empty()` heuristic proving brittle on mixed native+bitcode rlibs — it did not
(all 234.3 targets recovered correctly), so the dual-path order stands.
- **Files:** `packages/core/nros-sizes-build/src/lib.rs`. Regression test
  (`bitcode_probe.rs`) still green.

### 234.5 — Add a probe regression test  ✅
`packages/core/nros-sizes-build/tests/bitcode_probe.rs` builds `nros` with
`CARGO_PROFILE_RELEASE_LTO=fat` into a throwaway target dir, finds the bitcode
`libnros-*.rlib`, and asserts `extract_sizes` recovers `PUBLISHER_SIZE > 0` and
`EXECUTOR_SIZE > PUBLISHER_SIZE` — which for a bitcode rlib can only come from the
`llvm-nm` name-based fallback. `#[ignore]` (spawns a ~15s fat-LTO compile); run with
`cargo test -p nros-sizes-build --test bitcode_probe -- --ignored`. Passed in 14.79s.
- **Files:** `packages/core/nros-sizes-build/tests/bitcode_probe.rs`.

### 234.6 — Measure + record the size win  ✅
`logging-smoke-mps2-baremetal` (probe-consuming `thumbv7m-none-eabi` firmware, links a
full ELF via `rust-lld`) built `lto=off` vs `lto=fat`:

| section | lto=off | lto=fat | Δ |
|---|---|---|---|
| `.text` | 7488 | 5636 | **−1852 (−24.7%)** |
| `dec` (text+data+bss) | 7676 | 5912 | **−1764 (−23.0%)** |

Both link cleanly; `rust-lld` LTO-links the bitcode rlibs with no error. Larger
RMW-carrying firmware should see a bigger absolute win. The memory-noted Cyclone
`rust-lld` link hazard is a C-side cmake `ENABLE_LTO` setting, untouched by the Rust
profile flip.

## Acceptance

- ✅ `[profile.release]` no longer pins `lto = "off"` (now `"fat"`).
  `nros-fast-release` keeps its explicit `lto = "off"` *by design* (inherits release;
  override required to stay fast — see 234.2).
- ✅ nros-c / nros-cpp recover **byte-identical** opaque sizes under LTO vs the 234.1
  baseline on host + `thumbv7m` + `nuttx` (filesystem path); both link clean at
  `release` (fat LTO).
- ✅ A regression test guards the bitcode probe path (`bitcode_probe.rs`).
- ✅ `just rust-rtos-link-check` green under fat LTO (freertos + nuttx +
  threadx-linux); size win recorded. (Full `just ci` / `test-all` QEMU-boot matrix +
  zephyr-SDK lanes left to CI — the embedded link-symbol regression *class* is covered
  by the link-check gate above.)
- ✅ Issue 0023 → `resolved` (`resolved_in: Phase 234`), moved to
  `docs/issues/archived/`.

## Notes

- The `nros-c`/`nros-cpp` opaque buffers are sized from these probe values
  (`alignas(…) uint8_t storage_[…]`); a wrong size silently truncates the C/C++
  `_opaque` slot (cbindgen ships a `*_OPAQUE_U64S = 1` placeholder otherwise), so the
  234.3 equality check is load-bearing, not cosmetic.
- Option A from issue 0023 (force the *nested probe build* `lto=off` independent of
  the consumer profile, via `CARGO_PROFILE_RELEASE_LTO=false` on the nested `cmd`)
  remains a cheap belt-and-suspenders if 234.3 surfaces a fallback gap on a target —
  it keeps the probe rlib native while the firmware binary stays LTO'd.
- Cross-ref: issue 0023, Phase 87/118 (probe design), `Cargo.toml:612-638`.
