# Build UX Audit — user-project build profiling

- **Date:** 2026-06-16
- **Scope:** the build a *user* runs for their own nano-ros node project (not the
  repo's internal `just ci` / fixture matrix).
- **Companion:** the profiling feature this audit motivates is specified in
  `docs/superpowers/specs/2026-06-16-build-profiling-design.md` and tracked in
  `docs/roadmap/phase-251-build-profile-analyzer.md`.

## Why

nano-ros ships as an **external dependency** consumed through each RTOS framework's
own toolchain — `west build` (Zephyr), `cmake` + `rosidl_generate_interfaces()`,
`idf.py`/`espflash` (esp32), and `cargo` (native, esp32 bare-metal, cross targets).
The `nros` CLI deliberately covers only **setup** (toolchain/SDK provisioning) and
**codegen** (message + system generation); it does not own the build.

That split is good for layering, but it leaves the user's build a **black box for
time attribution**. A user who waits 40 s for `west build` has no breakdown of where
the time went (codegen vs compile vs link vs flash), no per-unit detail, and no signal
when a shared dependency is recompiled redundantly. This audit inventories what build
timing/UX exists today and what is missing, to scope a passive profiling tool.

## What exists today

### Internal fixture-matrix timing (repo dev only)
- `justfile` `build-test-fixtures` — a `run_stage()` helper stamps Unix epochs per
  platform slice and writes a TSV joblog (`stage, start_epoch, end_epoch,
  duration_seconds, status`) under `tmp/build-test-fixtures-latest/`.
- `scripts/build-all-jobserver.sh` — timestamped log dir + make/ninja version banner.

These time the **whole repo's** fixture sweep, not a single user project, and are
coarse (one row per platform slice).

### Codegen progress (partial)
- `packages/cli/cargo-nano-ros/src/workflow.rs` — an `indicatif` progress bar with an
  elapsed timer, **only** when generating bindings for more than one package in
  parallel. Single-package codegen is silent.

### What is NOT instrumented
- No per-stage timing for a single user build (codegen / compile / link / flash are
  one opaque run).
- No consumption of `cargo build --timings` or `--message-format=json`.
- No use of ninja's `.ninja_log` (west/cmake/idf all produce it).
- No progress/ETA during `nros setup` SDK downloads or git submodule fetch.
- No slow-spot attribution, cache-hit indication, or redundant-rebuild detection.

## Documented pain points

| Source | Pain |
| --- | --- |
| `docs/issues/0057-host-integration-tests-red-oom-and-skip-gating.md` | Fixture build fans out `NROS_BUILD_JOBS=8` cargo frontends; heavy codegen deps exceed CI RAM → kernel OOM-kills rustc → silent corrupt fixtures. No memory-aware job feedback. |
| `docs/roadmap/phase-226-fixture-build-orchestration-audit.md` §3 | Shared crates (`nros-c`, `nros-cpp`, heapless…) recompile **3×+** because standalone examples use isolated per-example `target/` dirs. Manifest supports `target_dir` pooling but it is not broadly used. |
| phase-226 §4 | Scheduler bypasses: GNU `parallel`, raw shell `&` jobs, static `NROS_*_JOBS` splits hidden from the jobserver → uneven CPU use. |
| phase-226 §5 | Lock contention: multiple cargo frontends serialize on registry/index/cache + rustup component locks. |
| `docs/development/codebase-audit-checklist.md` §F1 | CLI error-message quality + bootstrap friction; provisioning has no progress feedback. |

## Slow spots (evident)
- **Cold C library builds** — `libzenoh_pico.a`, CycloneDDS; large single units with no
  Rust-style incremental, often the dominant compile cost.
- **`west build`** — nested CMake/Ninja, serial by default to avoid races.
- **First-build codegen** — stale/missing bindings block the flow; silent for a single
  package.
- **SDK provisioning** — toolchain/QEMU/Zephyr downloads with no progress or ETA.

## How backends expose timing (the key finding)

Across the whole platform matrix, deep timing data collapses to **two artifact
formats**, both already produced by the native build with zero or one-flag opt-in:

| Artifact | Format | Backends covered | Opt-in |
| --- | --- | --- | --- |
| `build*/.ninja_log` | `start_ms end_ms mtime output cmdhash` per output | west (Zephyr), cmake (C/C++), idf.py (esp32-idf) | **none** — ninja always writes it |
| `target*/cargo-timings/cargo-timing-*.html` | embedded `UNIT_DATA` JSON (unit name, start, duration, rmeta) | cargo (native, esp32 bare-metal, all cross targets) | `cargo build --timings` |

Flash/`espflash` time is captured only as coarse wall-clock (no internal breakdown).

This is what makes a cross-backend profiler tractable: **two parsers cover everything.**

## Profiling options considered

1. **Build front-door** (`nros build --profile`) — nros owns the sequence, giving exact
   stage boundaries. **Rejected:** requires nros to drive every backend and forces a new
   build verb; conflicts with the setup+codegen-only scope and the external-dep model.
2. **Build wrapper** (`nros profile -- <cmd>`) — nros wraps an arbitrary build command.
   **Rejected:** still a build-adjacent nros verb; sniffing stages from a single wrapped
   process is fuzzy.
3. **Passive post-build analyzer** (chosen) — the build runs unchanged on its native
   toolchain; a separate read-only tool parses the artifacts above into a normalized
   profile and report. Coarse stage timing is always available; deep per-unit detail
   comes from the two parsers. No new build/test verb; nano-ros stays an external dep.

## Recommendation

Build a passive analyzer (`nros-build-profile`) driven by a `just profile <dir>`
recipe, with the parser binary also runnable standalone for external copy-out projects.
Two collectors (`ninja_log`, `cargo_timings`), a normalizer to a single `BuildProfile`,
a small data-driven diagnostics layer (cold-C-build, shared-crate-recompiled-N×,
isolated-`target/`, job-count-vs-RAM), and a reporter (terminal table + `--deep`
drill-down + `--json`). Design: the companion spec. Work items + acceptance: phase-251.

## Measured: the caching win (2026-06-15)

Used the shipped analyzer to quantify the dominant finding (Rust→C-FFI / zenoh-pico
cold builds) on `examples/native/rust/talker`. The repo wires a compiler cache only
*if present* (`RUSTC_WRAPPER := sccache` in the justfile, falling back to empty), and
**never** sets `CC=ccache` for the C side. On this host **sccache is not installed** and
`CC` is unwrapped → out of the box, clean builds cache nothing.

| build (`cargo clean` each time) | total | codegen stage | cache |
| --- | --- | --- | --- |
| **cold** (no cache) | 30.4 s | 21.8 s (72 %) — `zpico-sys` zenoh-pico C compile | — |
| **ccache-warm** (`CC="ccache cc"`) | 19.9 s | 4.0 s | 133/133 C hits |
| **sccache-warm** (`RUSTC_WRAPPER=sccache` + `CC="sccache cc"`) | **16.3 s** | 5.8 s | 271/272 hits (132 C + 139 Rust), 99.6 % |

**ccache (C only) cut a clean rebuild ~33 %; sccache (rustc + C) cut it ~46 %** (30.4 →
16.3 s). sccache wins more because one wrapper caches *both* the zenoh-pico C build and
the rustc dep compiles (`syn`, `zerocopy`, the `nros-*` crates). The zenoh-pico C compile
(`zpico-sys`, the single biggest cold unit at ~17 s) is cache-stable and goes to ~0 either
way.

Takeaways:
- A compiler cache roughly halves clean/CI build time, but **none is enabled by default**:
  sccache wasn't installed (justfile's `RUSTC_WRAPPER := sccache` fell back to empty), and
  the C build is never `CC`-wrapped.
- One wrapper covers both languages — `RUSTC_WRAPPER=sccache` + `CC`/`CXX="sccache cc"` is
  simpler than running sccache for Rust and ccache for C separately.
- **After caching, the bottleneck shifts to build-script *execution*** — nano-ros's own
  codegen tooling (`nros-cli-core`, `rosidl-codegen`, `zpico-sys` build-script *run*, not
  its C compiles) — which a compiler cache cannot touch. Next lever after caching would be
  codegen-output caching / fewer codegen re-runs, not faster compilation.
- Action: provision sccache via the SDK tier and export the wrapper + `CC`/`CXX` in the
  build env so fresh checkouts / CI / config-switches stop paying the full cold cost.
  Filed as issue #74.

### Embedded (esp32-c3, `cargo` + build-std, cross)

Same experiment on `examples/qemu-esp32-baremetal/rust/talker` (riscv32imc, build-std
core/alloc):

| build (`cargo clean` each) | total | cache |
| --- | --- | --- |
| **cold** (no cache) | 30.7 s | — |
| **sccache-warm** (`RUSTC_WRAPPER=sccache`) | **24.0 s** | 245 Rust hits, 9 misses |

Only **~22 %** here vs ~46 % on host, and the gap is instructive: `RUSTC_WRAPPER` caches
**rustc only**, so the cross **zenoh-pico C build** (compiled for riscv32 via the `cc`
crate) stays uncached and remains a large fixed cost. On embedded the C side is the bigger
share, but it is exactly where `CC`-wrapping is risky (cross toolchain) — so the safe
rustc cache yields a smaller embedded win. Caching the cross C compile (a per-target
`CC_<triple>="ccache …"`) is the additional embedded lever, to be done carefully per
target rather than via a global `CC`.
