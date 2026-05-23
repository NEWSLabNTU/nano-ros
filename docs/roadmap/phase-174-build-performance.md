# Phase 174 - Build performance

**Goal.** Cut `just build-all` wall-clock by maximizing cache reuse and
core utilization across the heterogeneous build (cargo + build-script
`cc` + ninja-via-west + cmake). Captures what landed plus the
remaining opportunities found while bringing `build-all` green on main.

**Status.** Pragmatic scope done. The safe build-system wins have
landed: sccache wiring, global parallelism budget, parallel Zephyr,
Zephyr incremental rebuilds, ninja-direct warm rebuilds, and the
unified jobserver from Phase 176. The remaining 174.A sysbuild/shared
image research was completed on 2026-05-24 and rejected as the wrong
lever for this fixture matrix. The remaining 174.B cross-config
picolibc dedup measurement is intentionally deferred.

**Priority.** P3 (ergonomics/CI throughput).

**Depends on.** none for the landed work. Phase 176 supplied the
jobserver path. SDK-prebuilt picolibc was verified already optimal for
SDK targets and unavailable for native_sim host-toolchain fixtures.

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

## Closed / Deferred Opportunities

### 174.A — zephyr per-example overhead — **landed; residual research deferred**

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

- [x] **fixture signature guard** (landed 2026-05-23). The ninja-direct
  fast path now records the configured fixture inputs beside each build
  dir (`board`, app source, RMW overlays, XRCE port, codegen tool,
  toolchain cache, `MAKE`, and compiler launcher). If any of those
  change, the next build uses `west build` once to refresh CMake instead
  of blindly reusing a stale `build.ninja`; unchanged dirs still take
  the sub-second ninja path.

- [x] **patch idempotence fix** (landed 2026-05-23). The Cortex-R
  zephyr-lang-rust patch guard now detects the actual inserted
  `CPU_AARCH32_CORTEX_R` token. Before this, `just zephyr
  build-fixtures` touched Zephyr's Rust Kconfig on every run, causing
  ninja to regenerate CMake and rebuild far more than the fixture
  changed.

**Deeper investigation (2026-05-21) — diminishing returns.**
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

Verdict: the clean, safe wins (pristine=auto + ninja-direct + fixture
signatures + sccache + the jobserver) are banked. Further cold-build
reduction needs a Zephyr sysbuild/shared-image rework + a clean
cyclonedds workspace. Deferred as a tracked research item rather than
hacked in.

**Final sysbuild/shared-image research (2026-05-24) — no implementation.**
The current fixture list has 55 Zephyr `native_sim/native/64` images:
19 zenoh, 18 xrce, and 18 cyclonedds. A narrow probe of
`build-rs-{talker,listener}-zenoh` after a clean pull reached the
same 1300-target Zephyr graph in **each** build dir and rebuilt/link-staged
`modules/picolibc/libc.a` separately. The probe then failed at the Rust
message-crate step because the local generated `examples/zephyr/rust/*/generated`
directories had been cleaned; that does not affect the build-graph
finding, because the duplicated kernel/picolibc work had already been
scheduled in both dirs.

Local Zephyr 3.7 sysbuild sources confirm why it does not solve this:

- `doc/build/sysbuild/index.rst` defines sysbuild as a higher-level layer
  that manages one or more Zephyr build systems/domains and emits one
  image per managed build system.
- `share/sysbuild/cmake/modules/sysbuild_extensions.cmake` implements
  `ExternalZephyrProject_Add()` with `ExternalProject_Add()`, a per-image
  `BINARY_DIR=${CMAKE_BINARY_DIR}/${APPLICATION}`, and later
  `ExternalZephyrProject_Cmake()` invokes CMake for each image with its
  own `-B${BINARY_DIR}` / `-S${SOURCE_DIR}`.
- `share/sysbuild/images/CMakeLists.txt` adds the main app as the first
  image, then additional bootloader/module/board/SoC images. It orders
  multiple image builds; it does not create a shared Zephyr kernel/libc
  artifact that several app images link against.

So converting the fixture sweep to sysbuild would mostly replace many
top-level `west build` invocations with one top-level sysbuild
invocation that still configures and builds many nested Zephyr images.
It might trim a little `west` process startup, but the expensive parts
remain: per-image devicetree/Kconfig/autoconf, generated include paths,
picolibc archive, Zephyr kernel archive, app archive, and final
`zephyr.elf`/native_sim executable link. Reusing those artifacts outside
the image build would fight Zephyr's normal dependency model because
they are compiled against each image's generated headers, config,
linker state, and app-selected modules. The safe reuse layer for that is
already sccache.

The only credible next cold-build win is not sysbuild; it is a
source-level fixture collapse: create fewer test-only Zephyr apps under
`packages/testing/` (not `examples/`) that can select talker/listener,
service, or action roles at runtime for a fixed board/RMW/language. That
could cut app image count, but it changes test fixture shape and would
need new runtime argument plumbing plus parity checks against the
standalone examples. Treat that as a new phase, not more 174.A build
glue.

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

**Status:** landed in Phase 176. `just build-all` auto-routes through
the jobserver when pinned make/ninja are available; set
`NROS_NO_JOBSERVER=1` to force the older static split.

## Notes

- The landed knob + parallel-zephyr + jobserver cover the practical
  parallelism wins. 174.A's remaining cold per-example configure/link
  cost is structural Zephyr work; sysbuild/shared-config-tree is the
  lever there.
- All `just/*.just` recipes read `${NROS_BUILD_JOBS:-N}` for their inner
  fan-out; never re-introduce a hardcoded `--jobs` constant without
  threading the budget through.
