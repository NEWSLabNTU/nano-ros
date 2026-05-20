# Phase 174 - Build performance

**Goal.** Cut `just build-all` wall-clock by maximizing cache reuse and
core utilization across the heterogeneous build (cargo + build-script
`cc` + ninja-via-west + cmake). Captures what landed plus the
remaining opportunities found while bringing `build-all` green on main.

**Status.** Partially done — sccache wiring + a global parallelism
budget + parallel zephyr have landed; the larger structural wins
(jobserver, SDK-prebuilt picolibc, per-example overhead) are open.

**Priority.** P3 (ergonomics/CI throughput).

**Depends on.** none for the landed work. The open items pull in
Phase 173 (jobserver) and Phase 67 / #67 (SDK-prebuilt picolibc).

## Landed

- **sccache active for all cargo** — `justfile` `RUSTC_WRAPPER :=
  command -v sccache`. It was inert only because sccache wasn't
  installed; the ~150 standalone example/fixture crates now share one
  compile cache instead of recompiling `nros` / `zenoh-pico` /
  `heapless` / … each. ~57% Rust hit rate observed.
- **sccache for zephyr C/C++** — `just/zephyr.just` routes the compiler
  through `CMAKE_*_COMPILER_LAUNCHER=sccache` (enabled by default;
  serial-build raciness was never about sccache). Lifted the C/C++ hit
  rate from ~11% to ~60% — picolibc/kernel objects now reused across
  same-RMW examples.
- **`SCCACHE_CACHE_SIZE=30G`** — the default 10 GiB evicted mid-sweep.
- **Global `NROS_BUILD_JOBS` budget** — one knob (default nproc) scales
  the whole build. `build-test-fixtures` runs the 7 cargo platforms in a
  divided pool and zephyr on a **solo full-budget track** (it's the long
  pole), re-exporting `budget/N` to each child. `NROS_BUILD_JOBS=8 just
  build-all` caps total concurrency at 8.
- **Parallel zephyr** — `BUILD_JOBS = budget/8` concurrent west builds ×
  `ninja = budget/BUILD_JOBS`; per-example `-d` dirs isolate them. Was
  serial 1×8 (≤8 cores even alone); now 4×8 on a 32-core host.

## Open opportunities

### 174.A — zephyr per-example overhead (biggest residual)

Each of ~21 zephyr fixtures pays, uncacheably and largely serially:
`west`/python startup (~3 s) + **cmake reconfigure** + codegen
(`nros_generate_interfaces`) + **link** (`zephyr.elf`). With
`pristine=always` (default) the build dir is wiped every time, forcing a
full reconfigure + ninja-graph eval. The *compile* is ~60% sccache-cached;
this configure/codegen/link tax is not.

Ideas:
- `pristine=auto` + **shared build dirs per RMW group** so one configured
  kernel+picolibc tree is reused across same-config examples instead of
  reconfiguring each.
- Zephyr **sysbuild** / a single multi-image build instead of N
  independent `west build`s.
- Cache codegen output (`nros_generate_interfaces`) across examples that
  share interfaces.

### 174.B — config-divergent cache misses (~40%)

The 3 RMW overlays (zenoh/xrce/cyclonedds) produce different
`autoconf.h`, so picolibc/kernel recompile **once per RMW config** then
cache-hit within it. Narrowing config divergence (or pre-staging a
per-config kernel/picolibc tree) would raise the 60% hit rate.

### 174.C — SDK-prebuilt picolibc (see #67)

For SDK cross toolchains (aarch64/arm/riscv64-zephyr-elf) use the
SDK's prebuilt picolibc (`CONFIG_PICOLIBC_USE_MODULE=n`) to skip the
~1300-object compile entirely. native_sim (host gcc) has no SDK
prebuilt → sccache stays the only lever there.

### 174.D — unified jobserver (see Phase 173)

One make-fifo jobserver shared across cargo + cc + ninja(≥1.13) + cmake,
replacing the static `NROS_BUILD_JOBS` outer×inner split with dynamic
allocation — frees the tail-platform from its fixed share. Needs ninja
≥1.13 + make ≥4.4. Note: helps the *parallel* axis, not 174.A's serial
configure/link tax.

## Notes

- The landed knob + parallel-zephyr give most of the easy win; 174.A
  (per-example configure/link) is now the dominant zephyr cost and is
  serial, so neither the knob nor Phase 173 addresses it — sysbuild /
  shared-config-tree is the lever there.
- All `just/*.just` recipes read `${NROS_BUILD_JOBS:-N}` for their inner
  fan-out; never re-introduce a hardcoded `--jobs` constant without
  threading the budget through.
