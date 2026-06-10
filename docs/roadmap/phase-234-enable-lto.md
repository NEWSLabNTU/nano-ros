# Phase 234 — Enable LTO (the size probe already reads bitcode)

**Goal.** Remove the workspace-wide `lto = "off"` pins so embedded release builds
get the link-time-optimization size/perf win. The constraint that forced them — the
opaque-size probe could only read native-object symbol byte sizes — is already
lifted: `nros-sizes-build` has a bitcode-aware `llvm-nm` reader, validated to
recover every size under `lto = "fat"`. This phase flips the profiles and proves the
probe consumers (`nros-c`, `nros-cpp`) get identical sizes across the target matrix.

**Status.** In progress (2026-06). Profiles flipped, probe validated byte-identical
on host + `thumbv7m-none-eabi`, regression test landed. Remaining acceptance gate:
the `nuttx` filesystem-fallback path, the embedded-link CI matrix, and the size-win
measurement (234.6). Resolves issue
[0023](../issues/0023-lto-disabled-for-size-probe.md).

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

### 234.3 — Validate sizes across the target matrix  🟡 (host + thumbv7m ✅, nuttx ⬜)
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
- **`armv7a-nuttx-eabihf`** ⬜ — custom-target JSON → the **filesystem-fallback**
  probe path (reads the *outer* LTO'd rlib — the path most at risk). Needs the
  custom-target toolchain; deferred to CI / out-of-band.
Any divergence (esp. a `0` or a placeholder `*_OPAQUE_U64S = 1`) is a fail.
- **Files:** none (verification); fixes land in 234.4 if a gap surfaces.

### 234.4 — Harden the fallback (as needed)  ⬜
Driven by 234.3 results:
- widen `saw_bitcode` detection beyond `BC\xC0\xDE` (Mach-O-embedded
  `\xDE\xC0\x17\x0B`) so non-Linux hosts gate correctly;
- fix the `lib.rmeta` skip — the member is `lib.rmeta/` (trailing slash), so
  `name_bytes == b"lib.rmeta"` never matches (harmless ELF parse waste today);
- if the `out.is_empty()` gate proves brittle on mixed native+bitcode rlibs, make
  the `llvm-nm` name-based path **primary** (it reads native *and* bitcode
  uniformly) with the `object` byte-size path as the no-`llvm-tools` fallback.
- **Files:** `packages/core/nros-sizes-build/src/lib.rs`.

### 234.5 — Add a probe regression test  ✅
`packages/core/nros-sizes-build/tests/bitcode_probe.rs` builds `nros` with
`CARGO_PROFILE_RELEASE_LTO=fat` into a throwaway target dir, finds the bitcode
`libnros-*.rlib`, and asserts `extract_sizes` recovers `PUBLISHER_SIZE > 0` and
`EXECUTOR_SIZE > PUBLISHER_SIZE` — which for a bitcode rlib can only come from the
`llvm-nm` name-based fallback. `#[ignore]` (spawns a ~15s fat-LTO compile); run with
`cargo test -p nros-sizes-build --test bitcode_probe -- --ignored`. Passed in 14.79s.
- **Files:** `packages/core/nros-sizes-build/tests/bitcode_probe.rs`.

### 234.6 — Measure + record the size win  ⬜
Diff a representative embedded binary (e.g. a `logging-smoke-*` or a QEMU example)
`lto=off` vs `lto=fat`; record the `.text`/total delta in the phase notes + issue
0023 resolution. The payoff that justifies the change.

## Acceptance

- `[profile.release]` / `nros-fast-release` no longer pin `lto = "off"`.
- nros-c / nros-cpp recover **byte-identical** opaque sizes under LTO vs the 234.1
  baseline on host + `thumbv7m` + `nuttx` (filesystem path).
- A regression test guards the bitcode probe path.
- `just ci` green; the size win is recorded.
- Issue 0023 → `resolved` (`resolved_in: Phase 234`), moved to `docs/issues/archived/`.

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
