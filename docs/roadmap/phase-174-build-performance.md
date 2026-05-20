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
Phase 176 (jobserver) and Phase 67 / #67 (SDK-prebuilt picolibc).

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
(`nros_generate_interfaces`) + **link** (`zephyr.elf`). The *compile* is
~60% sccache-cached; this configure/codegen/link tax is not.

- [x] **`pristine=auto`** (landed 2026-05-21). Each build dir is
  variant-unique (`build-<lang>-<ex>-<rmw>`, fixed board + overlay), so
  its config never changes — `auto` is safe and rebuilds incrementally
  instead of `always` wiping + full-rebuilding every run. Measured on
  `zephyr/rust/talker` (zenoh): **cold 19.97 s → warm no-change rebuild
  6.10 s (~3.3×)**. `NROS_ZEPHYR_PRISTINE=always` forces a clean rebuild.
  Repeated `just zephyr build-fixtures` (dev inner-loop / warm-dir CI)
  now goes incremental.

- [x] **ninja-direct incremental** (landed 2026-05-21). `west build`
  ALWAYS re-runs the full cmake configure (devicetree regen,
  Rust-target detect, codegen) — ~4 s even on a no-op, which is the bulk
  of the warm `-p auto` time. When a build dir is already configured,
  the recipe now builds with `ninja -C <dir>` directly; ninja
  regenerates `build.ninja` itself only when a cmake input
  (CMakeLists / prj.conf) actually changed, so it stays correct.
  Measured on `zephyr/rust/talker` zenoh, no-change rebuild:
  **west `-p auto` 6.10 s → ninja-direct 0.28 s** (0 reconfigure).
  Combined with pristine=auto: cold 20 s → no-change 0.28 s/example.
  Under the fifo jobserver ninja inherits the pool; else it gets the
  per-build ninja-jobs budget. `pristine=always` still forces west.

**Deeper investigation (2026-05-21) — diminishing returns, deferred.**
With the warm same-dir rebuild already at 0.28 s, the residual cost is
the **cold first build of each fresh build dir**, which decomposes as:
per-dir cmake configure (~4 s: devicetree regen + Rust-target detect)
+ picolibc (934 TUs — sccache cache-hits *within* a config, but each
app still fetches + relinks its own copy) + app + link. Both remaining
levers fight Zephyr's **one-app-one-image** model:

- **Codegen caching** (`nros_generate_interfaces`) — marginal: the
  fixtures use tiny interfaces (`std_msgs/Int32`), so regen is sub-second;
  not worth a cross-build cache.
- **Shared kernel+picolibc / sysbuild** — the only real cold win, but
  Zephyr links picolibc + kernel **per app** by design; sharing one
  `libc.a`/kernel across the 6 examples of a config is a sysbuild-grade
  restructure (research-level), risky, and **measurement-blocked** by the
  wedged local cyclonedds-zephyr workspace (Phase 171.0.d). The per-dir
  configure (~4 s) is likewise per-build-dir in Zephyr's model with no
  cheap share.

Verdict: the clean, safe wins (pristine=auto + ninja-direct + sccache +
the jobserver) are banked; further cold-build reduction needs a Zephyr
sysbuild rework + a clean cyclonedds workspace. Deferred as a tracked
research item rather than hacked in.

### 174.B — config-divergent cache misses — **investigated, deferred** (2026-05-21)

Measured (talker, native_sim): the RMW overlays select **different
libc footprints**, not just different `autoconf.h`. zenoh builds the
full picolibc (934 TUs; zenoh-pico needs full libc); **xrce builds 0
picolibc objects** (194 targets total, minimal libc). So the picolibc
compile cost is the full-picolibc configs only (zenoh + cyclonedds),
not xrce.

Within a config, picolibc already dedups across examples via sccache
(same autoconf → same preprocessed TU → cache hit) — that's most of the
landed 60% C/C++ hit rate. The open question is **cross-config dedup**:
do zenoh and cyclonedds (both full-picolibc) share picolibc objects?
They likely don't fully — the cyclonedds overlay adds
`COMMON_LIBC_MALLOC_ARENA_SIZE` / thread configs that perturb the
autoconf the picolibc TUs see. Clean measurement is **blocked**: the
local zephyr workspace's cyclonedds builds are wedged by the
`nsos_adapt.c` duplicate-case (Phase 171.0.d — fixed upstream only on a
clean tree; this workspace is already polluted, needs re-pristine).

Achievable win is bounded + risky (narrowing the cyclonedds overlay's
picolibc-relevant config to match zenoh's, to dedup ~934 TUs once —
intersects cyclonedds runtime correctness). Deferred behind a clean
cyclonedds-zephyr workspace; lower ROI than the landed 174.A / 176 wins.

### 174.C — SDK-prebuilt picolibc — **already optimal, no change** (verified 2026-05-21)

Investigated: nothing to do. Zephyr's `lib/libc/picolibc/Kconfig`
already does `default PICOLIBC_USE_TOOLCHAIN if … "$(TOOLCHAIN_HAS_PICOLIBC)" = "y"`,
and the SDK cross toolchains ship prebuilt picolibc
(`scripts/zephyr/sdk/.../{aarch64,arm,riscv64}-zephyr-elf/.../picolibc/.../libc.a`
— 2/23/24 multilib `libc.a`). No nros config forces
`PICOLIBC_USE_MODULE` (grepped examples + `zephyr/Kconfig` +
`zephyr/CMakeLists.txt` + recipes). So the SDK cross targets
(FVP-aemv8r = aarch64-zephyr-elf, S32Z = arm-zephyr-eabi) **already
link the prebuilt** — they never compile the picolibc module.

The fixture sweep, however, is **all `native_sim/native/64`** (host
gcc), which has no picolibc → `TOOLCHAIN_HAS_PICOLIBC=n` →
`PICOLIBC_USE_MODULE` is the only option (the host has no prebuilt to
link). So the prebuilt route can't touch the dominant native_sim cost;
it's covered by sccache (~60% C hit) + ninja-direct (warm skip) +
pristine=auto. The only way to drop native_sim's picolibc compile is a
libc SWAP (host glibc / minimal-libc), which is 174.B territory and
intersects the cyclonedds picolibc requirement — out of scope here.

### 174.D — unified jobserver (see Phase 176)

One make-fifo jobserver shared across cargo + cc + ninja(≥1.13) + cmake,
replacing the static `NROS_BUILD_JOBS` outer×inner split with dynamic
allocation — frees the tail-platform from its fixed share. Needs ninja
≥1.13 + make ≥4.4. Note: helps the *parallel* axis, not 174.A's serial
configure/link tax.

## Notes

- The landed knob + parallel-zephyr give most of the easy win; 174.A
  (per-example configure/link) is now the dominant zephyr cost and is
  serial, so neither the knob nor Phase 176 addresses it — sysbuild /
  shared-config-tree is the lever there.
- All `just/*.just` recipes read `${NROS_BUILD_JOBS:-N}` for their inner
  fan-out; never re-introduce a hardcoded `--jobs` constant without
  threading the budget through.
