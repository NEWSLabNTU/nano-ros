# Phase 226 — Fixture Build Orchestration Audit

**Goal.** Replace the remaining GNU parallel and ad-hoc shell fan-out in
`just <platform> build-fixtures` / `just build-test-fixtures` with a
make-scheduled fixture graph, then reduce repeated compilation by giving
fixture builds shared caches where the build configuration is compatible.

**Status.** INVESTIGATED. Created 2026-06-07 after a focused audit of
the fixture build scripts. No implementation in this phase document yet.

**Priority.** P1. Fixture prebuilds are now a normal part of the full
verification workflow. The current path is slow, recompiles shared
packages many times, and does not consistently use the Phase 176
jobserver design that `just setup` now provisions.

**Depends on.**

- Phase 176 unified jobserver build orchestration.
- Phase 181 fixture manifest SSOT.
- Phase 225 workspace fixture migration.

---

## 1. Current Symptoms

Observed during `just <platform> build-fixtures`:

- Shared crates such as `nros-c` compile repeatedly, often once per
  standalone fixture package or CMake build dir.
- Multiple Cargo and CMake frontends run under GNU parallel or raw shell
  background jobs, so they are scheduled outside the make jobserver.
- CPU utilization is uneven: some phases oversubscribe, while others
  leave cores idle behind static frontend caps, Cargo locks, rustup
  locks, or serial shell loops.

The symptoms are consistent with the current recipes. They are not a
single Cargo bug.

---

## 2. Existing Scheduler Paths

### 2.1 `build-all` has the make jobserver path

`just build-all` auto-routes to `just build-all-jobserver` when the
pinned tools are present:

- `third-party/make/make` must be GNU make 4.4.
- `third-party/ninja/ninja` must be available.
- `scripts/build-all-jobserver.sh` exports `NROS_JOBSERVER=1`, prepends
  pinned make/ninja to `PATH`, prefetches Cargo state, generates
  bindings, and runs:

```sh
make -j"$NROS_BUILD_JOBS" --jobserver-style=fifo -f build-all.mk
```

`build-all.mk` is the only broad fixture path that uses make as the
outer scheduler today. Its targets call `just <platform> build-fixtures`
under a shared fifo jobserver.

### 2.2 Direct platform fixture builds do not use make

Direct commands such as `just qemu build-fixtures`, `just native
build-fixtures`, `just freertos build-fixtures`, and the non-jobserver
`just build-test-fixtures-leaves` path still schedule work with:

- GNU parallel in `scripts/build/fixtures-build.sh`.
- GNU parallel in several platform recipes.
- Raw `&` / `wait` background loops in native and Zephyr fixture paths.
- Static `NROS_BUILD_JOBS`, `NROS_CARGO_FRONTENDS`,
  `NROS_CMAKE_FRONTENDS`, `NROS_ZEPHYR_BUILD_JOBS`, and
  `NROS_ZEPHYR_NINJA_JOBS` splits.

This is the main mismatch with Phase 176. The design says make should
replace GNU parallel as the scheduler, but that replacement currently
applies only to the `build-all` wrapper, not to platform-scoped fixture
builds.

### 2.3 Jobserver mode serializes too much inside platform recipes

Most recipes detect `NROS_JOBSERVER=1` and then avoid GNU parallel by
running a serial launcher loop:

```sh
for dir in "${dirs[@]}"; do
    build_one "$dir"
done
```

That is safe under `build-all.mk`, because make runs multiple platform
targets concurrently. It is poor for a direct single-platform fixture
build: only one Cargo/CMake/West frontend runs at a time, so any serial
phase or lock wait leaves cores idle.

The desired future model is not "serial shell loop under
`NROS_JOBSERVER=1`". It is "make owns every independent fixture leaf".

---

## 3. Repeated Compilation Findings

### 3.1 Standalone examples use isolated Cargo target dirs

Many fixtures are intentionally standalone copy-out examples. The build
loop runs one Cargo frontend per example directory:

```sh
cd examples/<platform>/rust/<role>
cargo build ...
```

Most of those examples have their own local `target/`. Cargo therefore
sees separate target caches and recompiles common path dependencies,
including shared nano-ros crates.

This is visible in:

- `just/qemu-baremetal.just::build-fixtures`
- `just/stm32f4.just::build-fixtures`
- `scripts/build/fixtures-build.sh` for manifest-driven Rust rows
- `just/esp32.just::build-qemu` and `build-logging-smoke`

The examples should remain standalone when users build them manually,
but fixture prebuilds can safely override `--target-dir` when rows are
configuration-compatible.

### 3.2 The manifest supports `target_dir`, but it is not used broadly

`examples/fixtures.toml` already supports per-fixture `target_dir`.
Some native feature/RMW variants use it:

- `target-tls`
- `target-safety`
- `target-zero-copy`
- `target-zenoh`
- `target-xrce`
- `target-cyclonedds`

But several large plain fixture groups, including qemu bare-metal and
stm32f4 rows, do not set a shared fixture target dir. They therefore
pay repeated dependency compilation even when target triple, profile,
features, env, RMW, and generated inputs are identical.

### 3.3 CMake/Corrosion fixtures need measurement before cache changes

C and C++ examples build through independent CMake build directories.
Each build dir can own a separate Corrosion Cargo target tree, so the
output may show `nros-c` / `nros-cpp` compiling more than once.

`scripts/build/cmake-incremental.sh` preserves CMake build dirs when the
generator is stable, which helps warm rebuilds. It does not solve the
cross-fixture cache split: each CMake example still has its own build
directory and often its own Corrosion cargo cache.

However, broad target-dir sharing for `nros-c` / `nros-cpp` is not
obviously correct. The CMake definitions map to different Cargo
features and target triples:

- POSIX / ThreadX Linux host builds use `std`.
- FreeRTOS, ThreadX RV64, and ESP-IDF use `alloc` / `panic-halt` and
  different target triples.
- NuttX has special handling and avoids the normal CMake/Corrosion
  `nros-c` path.
- Zephyr builds through west/CMake and board-specific target triples.

The `packages/core/nros-c/CMakeLists.txt` comment is explicit:
`nros-c` is RMW-agnostic at the Cargo level, but the result is still one
`libnros_c.a` per target triple. `nros-cpp` has the same platform
feature split. Sharing across target triples would at best create Cargo
lock contention and at worst mix incompatible artifacts.

The current intended accelerator for repeated C/C++ compilation is
sccache:

- root `justfile` exports `RUSTC_WRAPPER=sccache` when present, covering
  Cargo/rustc work including Corrosion-launched Cargo;
- Zephyr routes C/C++ compilers through
  `CMAKE_C_COMPILER_LAUNCHER=sccache` and
  `CMAKE_CXX_COMPILER_LAUNCHER=sccache`;
- CycloneDDS self-provisioning also sets C/C++ compiler launchers to
  sccache when available.

Therefore the immediate task is to measure C/C++ fixture efficiency:
how often `nros-c` / `nros-cpp` really recompile, what sccache hit rate
we get, and which build dirs cause misses. Cache sharing should be a
follow-up only for same-target, same-feature groups that sccache does
not already handle well.

### 3.4 Native Cyclone C/C++ fixtures force clean rebuilds

`just/native.just::build-fixture-extras` is the worst current offender.
For native C/C++ Cyclone cells it runs 12 background CMake builds and
deletes each build dir first:

```sh
rm -rf build-cyclonedds
cmake -S . -B build-cyclonedds ...
cmake --build build-cyclonedds --target ...
```

That defeats incrementality and guarantees repeated configure/build
work. It also schedules the builds with raw shell background jobs
outside both GNU parallel and make.

---

## 4. Parallelism Findings

### 4.1 GNU parallel is still on the fixture path

The following paths still call GNU parallel for fixture or fixture-like
work:

- `justfile::build-test-fixtures-leaves`
- `justfile::build-example-extras`
- `scripts/build/fixtures-build.sh`
- `just/native.just::build-examples`
- `just/qemu-baremetal.just::build-fixtures`
- `just/stm32f4.just::build-fixtures`
- `just/freertos.just::build-fixture-extras`
- `just/nuttx.just::build-fixtures`
- `just/threadx-linux.just::build-fixture-extras`
- `just/threadx-riscv64.just::build-fixture-extras`

GNU parallel is now an avoidable dependency. `just setup` installs the
new make, and even without the pinned fifo jobserver make can still
schedule targets with normal make parallelism. The remaining GNU
parallel use should be removed from fixture orchestration.

### 4.2 Direct mode uses static split heuristics

`scripts/build/cargo.sh` defaults:

- Cargo frontends to `NROS_BUILD_JOBS` outside jobserver mode.
- Cargo frontends to `4` in jobserver mode.
- CMake frontends to `NROS_BUILD_JOBS` or `4`, depending on mode.

The root `build-test-fixtures-leaves` path further splits:

- Zephyr gets a solo full-budget track.
- The other platforms run through GNU parallel with `outer=4`.
- Each child receives `NROS_BUILD_JOBS=budget/outer`.

That model can oversubscribe during overlap and underutilize during the
tail. It also duplicates scheduler policy across recipes.

### 4.3 Explicit `-j` and `CMAKE_BUILD_PARALLEL_LEVEL` remain outside jobserver

Several direct paths set explicit CMake/Ninja parallelism when
`NROS_JOBSERVER` is not present:

- `CMAKE_BUILD_PARALLEL_LEVEL="${NROS_BUILD_JOBS:-8}"` in native,
  FreeRTOS, NuttX, and ThreadX fixture extras.
- Zephyr computes `NROS_ZEPHYR_BUILD_JOBS` and
  `NROS_ZEPHYR_NINJA_JOBS`, then runs `ninja -j "$ninja_jobs"` outside
  jobserver mode.

Those knobs made sense for static scheduling. They should disappear
from make-scheduled fixture leaves. A make-provided jobserver should be
the only parallelism budget; pure make fallback should still own the
outer target graph.

### 4.4 Raw background jobs bypass all scheduler accounting

Native Cyclone and Zephyr use shell background jobs:

- Native: two pure-Cargo Cyclone Rust builds, then 12 C/C++ Cyclone
  CMake builds.
- Zephyr: when `NROS_ZEPHYR_BUILD_JOBS > 1`, entries are launched with
  `&` and tracked in a shell array.

These jobs are invisible to make's target scheduler. They also consume
an implicit token incorrectly when run under a recipe that is already a
make jobserver client. Future fixture graph generation should express
each of these as make targets instead.

---

## 5. Lock and Cold-Start Findings

Multiple independent Cargo frontends can contend on shared state:

- Cargo registry/index/cache locks.
- Git dependency/cache locks.
- Rustup component downloads and installs.
- Generated code or package sync outputs when more than one row touches
  the same package directory.

The NuttX recipe already works around one concrete case by serializing a
rustup component warm-up before parallel `-Z build-std` fixture builds,
and by forcing `NROS_CARGO_FRONTENDS=1` for NuttX C/C++ fixtures.

`scripts/build-all-jobserver.sh` also prefetches the root workspace and
standalone manifests before broad fan-out. Direct platform fixture
builds do not consistently get that same prefetch phase.

---

## 6. Target Direction

### 6.1 Make should be the fixture scheduler

Introduce a fixture make driver for:

- direct `just <platform> build-fixtures`;
- root `just build-test-fixtures`;
- `just build-example-extras` where it participates in full builds.

The driver should generate or include make targets for independent
fixture leaves. Leaves should be small enough to schedule:

- one Cargo fixture row;
- one CMake fixture row;
- one Zephyr board/source/RMW build;
- one workspace fixture;
- one platform preflight or serial prerequisite.

When pinned GNU make 4.4 and Ninja 1.13 are available, run the graph
under:

```sh
make -j"$NROS_BUILD_JOBS" --jobserver-style=fifo
```

When the pinned fifo-capable make is absent, fall back to ordinary make
parallelism rather than GNU parallel. The fallback does not give Ninja a
fifo jobserver, but it still removes GNU parallel and centralizes the
outer scheduler.

### 6.2 Do not hide concurrency in recipes

Once fixture leaves are make targets, leaf commands should not spawn
their own leaf fan-out with:

- GNU parallel;
- raw `&` / `wait`;
- explicit `cmake --build --parallel`;
- explicit `ninja -j`;
- `CMAKE_BUILD_PARALLEL_LEVEL`;
- Cargo `-j` / `CARGO_BUILD_JOBS`.

Cargo, build-script `cc`, CMake generators, Ninja, and sub-make should
inherit the jobserver when available. Pure make fallback should schedule
the outer graph and run each leaf with conservative inner defaults.

### 6.3 Keep fixture-only cache sharing separate from user example shape

Examples should remain copy-out standalone projects. Fixture builds may
override target/build dirs because they are an internal staging path.

Rust fixture rows can share a target dir only when these inputs match:

- target triple;
- profile;
- RMW/features/default-feature state;
- relevant env;
- generated package state;
- toolchain/nightly/build-std requirements;
- platform SDK paths that affect build scripts.

CMake/Corrosion fixture rows need a separate efficiency audit before
changing cache layout. Since `nros-c` / `nros-cpp` are built for
different target triples and platform feature sets, broad target-dir
sharing is not a safe default. Prefer these in order:

1. keep relying on sccache when it gives high hit rates;
2. remove clean rebuilds and hidden fan-out that defeat incrementality;
3. consider same-target, same-feature shared target dirs only when
   measured data shows repeated real compilation not covered by
   sccache;
4. consider a fixture-stage prebuilt nano-ros C/C++ library only if it
   does not break the source-tree `add_subdirectory()` consumption model.

### 6.4 Preserve required serial preflights

Some setup work should remain serialized before the make graph fans out:

- root and standalone Cargo prefetch;
- `nros` CLI/codegen availability checks;
- Rustup component warm-up for build-std platforms;
- platform SDK/kernel provisioning such as NuttX kernel export;
- codegen/package sync when multiple fixtures would write the same
  generated output.

These should become explicit make prerequisites, not hidden side effects
inside many leaf targets.

---

## 7. Work Items

### Parallel Wave 1 — Investigation Split

Started 2026-06-07. Scope was read-only investigation plus phase-doc
updates; no build-script behavior changes in this wave.

- [x] Fixture graph inventory: enumerate every current fixture leaf and
      mark manifest-driven versus ad-hoc discovery.
- [x] GNU parallel / raw fan-out audit: classify every remaining
      scheduler bypass in fixture paths.
- [x] Rust target-dir grouping audit: identify safe same-config
      fixture-only `--target-dir` groups and rows that must stay
      isolated.
- [x] C/C++ efficiency measurement design: define sccache and build-log
      measurements before changing Corrosion cache layout.
- [x] Native Cyclone cleanup plan: replace raw background CMake loops
      and clean rebuilds with explicit fixture leaves.
- [x] Make driver shape: design a platform/full-matrix make driver with
      fifo jobserver path and ordinary make fallback.

Wave 1 result: the best first implementation slice is **native Cyclone
C/C++ fixture cleanup**. It is local, removes a raw background loop,
stops deleting `build-cyclonedds`, and moves 12 CMake cells into the
manifest path that already exists. The broader make driver should follow
once the fixture graph generator can emit native + Zephyr leaves.

### Wave 1 Findings — Fixture Graph

Root `just build-test-fixtures` starts at `justfile:565` and currently
builds:

- `generate-bindings` preflight (`justfile:1561`);
- POSIX zenoh staticlib fixture (`justfile:702`);
- Zephyr fixtures through direct `just zephyr build-fixtures`
  (`justfile:650`, `just/zephyr-ci.just:22`);
- `native`, `qemu`, `freertos`, `nuttx`, `threadx_linux`,
  `threadx_riscv64`, and `stm32f4` through direct platform
  `build-fixtures` fan-out (`justfile:661`).

Manifest-driven fixture leaves already cover:

- `native`: Rust rows, C rows, C++ rows, and four workspace rows
  (`examples/fixtures.toml:56`, `:106`, `:717`;
  `just/native.just:109`, `:143`, `:214`);
- `freertos`: six Rust + six C + six C++ zenoh role rows
  (`examples/fixtures.toml:492`, `:873`; `just/freertos.just:88`,
  `:186`);
- `nuttx`: six Rust + six C + six C++ zenoh role rows, though the
  direct recipe still manually loops Rust role dirs inside
  `build-fixtures` (`examples/fixtures.toml:551`, `:950`;
  `just/nuttx.just:131`, `:186`);
- `threadx-linux`: six Rust + C/C++ zenoh rows plus gated Cyclone C/C++
  rows (`examples/fixtures.toml:586`, `:1027`, `:1180`);
- `threadx-riscv64`: six Rust + C/C++ zenoh rows plus Cyclone
  talker/listener C/C++ rows (`examples/fixtures.toml:646`, `:1104`,
  `:1256`).

Important ad-hoc leaves still outside the manifest:

- Zephyr matrix generated from `scripts/build/fixture-matrix.sh:4` and
  built in `just/zephyr-ci.just:248`.
- QEMU bare-metal `find examples/qemu-arm-baremetal/**/Cargo.toml`
  plus manual bench/bin list (`just/qemu-baremetal.just:132`, `:135`).
- Native Cyclone Rust and native Cyclone C/C++ loops
  (`just/native.just:194`, `:224`).
- FreeRTOS, ThreadX, NuttX, ESP32 logging-smoke/image-packaging leaves
  (`just/freertos.just:131`, `just/threadx-linux.just:111`,
  `just/threadx-riscv64.just:130`, `just/esp32.just:105`, `:130`).
- STM32F4 hard-coded Rust list omits manifest `talker-embassy`
  (`just/stm32f4.just:43`, `examples/fixtures.toml:460`).

### Wave 1 Findings — Scheduler Bypasses

Highest-priority make-target candidates:

- `justfile:587` through `:664`: root fixture fan-out still uses a raw
  Zephyr background job, GNU parallel platform pool, and `wait`.
- `just/zephyr-ci.just:140` through `:507`: Zephyr splits build/ninja
  jobs, sets `CMAKE_BUILD_PARALLEL_LEVEL`, runs `ninja -j`, and uses raw
  background jobs.
- `just/native.just:194` through `:238`: native Cyclone Rust and C/C++
  fixtures use raw background jobs; C/C++ also deletes build dirs.

Fixture paths that still use GNU parallel or static frontend knobs:

- `scripts/build/fixtures-build.sh:35`, `:41`;
- `scripts/build/cargo.sh:76`, `:92`;
- `scripts/build/workspace-fixtures-build.sh:37`;
- `just/qemu-baremetal.just:124`, `:176`;
- `just/stm32f4.just:41`, `:58`;
- `just/freertos.just:120`, `:138`;
- `just/nuttx.just:125`, `:146`, `:186`;
- `just/threadx-linux.just:76`, `:83`;
- `just/threadx-riscv64.just:117`, `:142`;
- `just/native.just:159`.

Serial preflights that should become explicit make prerequisites:

- parent `nros ws sync` capability checks
  (`scripts/build/fixtures-build.sh:75`, `just/qemu-baremetal.just:143`);
- native Rust fixture codegen sync loop (`just/native.just:118`);
- NuttX kernel build and rustup warm-up (`just/nuttx.just:108`, `:114`).

### Wave 1 Findings — Rust Target-Dir Groups

Safe or likely-safe same-config Rust grouping candidates:

- Native default host fixtures and default bench rows
  (`examples/fixtures.toml:106`, `:348`), excluding feature/env/RMW
  variants.
- Native feature/RMW groups: TLS (`:199`), safety (`:214`), zenoh RMW
  (`:239`), XRCE (`:293`). The zenoh talker adds `param-services`, so
  either keep it separate or accept feature-union rebuild behavior.
- QEMU ARM bare-metal normal role fixtures
  (`examples/fixtures.toml:382` through `:430`) sharing
  `thumbv7m-none-eabi`; keep serial/XRCE and bench/bin extras separate.
- FreeRTOS zenoh Rust roles (`examples/fixtures.toml:492` through
  `:544`) sharing target/features/profile.
- NuttX Rust roles (`examples/fixtures.toml:551` through `:579`)
  sharing forced release/build-std setup; keep separate from C/C++
  Corrosion paths.
- ThreadX Linux zenoh Rust roles (`examples/fixtures.toml:586` through
  `:638`).
- ThreadX RV64 zenoh Rust roles (`examples/fixtures.toml:646` through
  `:698`) with shared `THREADX_CONFIG_DIR` / `NETX_CONFIG_DIR`.
- QEMU ESP32 bare-metal talker/listener (`examples/fixtures.toml:1313`
  through `:1323`), separate from real ESP32 rows.

Rows to keep isolated for now:

- Native `target-tls`, `target-safety`, `target-zero-copy`, and
  `target-large-buf` variants (`examples/fixtures.toml:199`, `:214`,
  `:229`, `:368`) unless grouped within their exact feature/env set.
- STM32F4 until its direct loop is reconciled with the manifest and
  per-example config/env hashing (`just/stm32f4.just:43`).
- QEMU bare-metal bench/bin extras and generated-package crates
  (`just/qemu-baremetal.just:129`).
- Real ESP32 rows (`examples/fixtures.toml:1301`) and ThreadX RV64
  Cyclone Rust/CMake hybrid (`just/threadx-riscv64.just:236`).

Implementation caveat: `target_dir` is currently emitted raw by
`scripts/build/fixtures-manifest.py:54`, and Cargo runs after
`cd "$dir"` in `scripts/build/fixtures-build.sh:116`. Repo-root-relative
fixture target dirs need builder support, or awkward per-row relative
paths such as `../../../../target/fixtures/<group>`.

### Wave 1 Findings — C/C++ Measurement

Use the existing stale-probe signal before changing Corrosion cache
layout: real rebuilds are C/C++ object/link lines plus Cargo
`Compiling ...` lines (`scripts/test/cmake-fixture-stale.sh:5`, `:34`).

Recommended measurement loop:

```sh
source ./activate.sh
export XDG_RUNTIME_DIR=/tmp
export NROS_BUILD_JOBS="${NROS_BUILD_JOBS:-8}"
mkdir -p tmp/phase226-cxx-eff
sccache --stop-server || true
sccache --zero-stats || true

for cell in \
  "native c zenoh" "native c xrce" "native cpp zenoh" "native cpp xrce" \
  "threadx-linux c zenoh" "threadx-linux cpp zenoh" \
  "threadx-linux c cyclonedds" "threadx-linux cpp cyclonedds" \
  "threadx-riscv64 c zenoh" "threadx-riscv64 cpp zenoh" \
  "threadx-riscv64 c cyclonedds" "threadx-riscv64 cpp cyclonedds"
do
  set -- $cell
  platform=$1 lang=$2 rmw=$3 tag="${platform}-${lang}-${rmw}"
  sccache --zero-stats || true
  CARGO_LOG=cargo::core::compiler::fingerprint=info \
    scripts/build/fixtures-build.sh "$platform" "$lang" "$rmw" \
    >"tmp/phase226-cxx-eff/${tag}.log" 2>&1
  sccache --show-stats >"tmp/phase226-cxx-eff/${tag}.sccache.txt" 2>&1 || true
done
```

Summarize logs:

```sh
for log in tmp/phase226-cxx-eff/*.log; do
  tag=${log##*/}; tag=${tag%.log}
  printf '%s\t' "$tag"
  printf 'nros-c=%s\t' "$(grep -cE 'Compiling nros-c v|Compiling nros_c v' "$log" || true)"
  printf 'nros-cpp=%s\t' "$(grep -cE 'Compiling nros-cpp v|Compiling nros_cpp v' "$log" || true)"
  printf 'cxx_objs=%s\t' "$(grep -cE 'Building (C|CXX|ASM) object' "$log" || true)"
  printf 'links=%s\t' "$(grep -cE 'Linking (C|CXX|CXX shared)' "$log" || true)"
  printf 'fingerprint=%s\n' "$(grep -c 'cargo::core::compiler::fingerprint' "$log" || true)"
done | column -t
```

Bucket results by target triple. `nros-c` should build once per target
triple, not once per RMW (`packages/core/nros-c/CMakeLists.txt:17`,
`:103`). `nros-cpp` has the same Corrosion staticlib import
(`packages/core/nros-cpp/CMakeLists.txt:77`) with later RMW compile
definitions (`:142`). The sccache anchors are `justfile:7`,
`just/zephyr-ci.just:388`, and
`packages/dds/nros-rmw-cyclonedds/cmake/ProvideCycloneDDS.cmake:41`.

### Wave 1 Findings — Native Cyclone First Slice

Retire the raw native C/C++ Cyclone loop at `just/native.just:223`
through `:238`. It backgrounds 12 CMake builds, deletes
`build-cyclonedds`, and suppresses output.

Plan:

- Add 12 native Cyclone C/C++ rows to `examples/fixtures.toml` near the
  existing native C/C++ rows (`examples/fixtures.toml:717`):
  C and C++ `talker`, `listener`, `service-server`, `service-client`,
  `action-server`, `action-client`.
- Use `platform = "native"`, `lang = "c"` / `"cpp"`,
  `rmw = "cyclonedds"`, and `dir = "examples/native/<lang>/<case>"`.
- Let `scripts/build/fixtures-manifest.py:279` default the build dir to
  `build-cyclonedds`; no explicit target is needed for those role rows.
- Move `-DNROS_RMW_CYCLONEDDS_MSG_TO_IDL=${repo_root}/scripts/cyclonedds/msg_to_cyclone_idl.py`
  from the raw loop into `NROS_CMAKE_EXTRA_DEFS`, alongside the current
  native C/C++ defs.
- Replace inline C/C++ calls with manifest leaves:
  `fixtures-build.sh native c`, `native cpp`, `native c cyclonedds`,
  and `native cpp cyclonedds`.
- Do not delete `build-cyclonedds`; `scripts/build/cmake-incremental.sh:25`
  already handles generator changes.

Keep serial: native tool/codegen preflight and scoped
`NROS_CMAKE_EXTRA_DEFS`. The native pure-Cargo Cyclone Rust talker/listener
block is separate and can wait for the make-driver conversion.

### Wave 1 Findings — Fixture Make Driver Shape

Add `scripts/build/fixture-make-driver.sh <platform|all>` as the
fixture-only scheduler. It should generate a per-run makefile under:

```text
tmp/fixture-make-<scope>-<timestamp>-<pid>/fixtures.mk
```

and symlink:

- `tmp/build-test-fixtures-latest` for `all`;
- `tmp/build-fixtures-<platform>-latest` for platform scope.

Mode selection:

```sh
if inherited fifo jobserver exists:
  exec third-party/make/make -f "$mk" all
elif third-party/make/make is GNU make 4.4 and third-party/ninja/ninja exists:
  exec env -u MAKEFLAGS -u CARGO_MAKEFLAGS \
    NROS_JOBSERVER=1 NROS_BUILD_JOBS="$n" NROS_BUILD_LOG_DIR="$log_dir" \
    PATH="$repo/third-party/make:$repo/third-party/ninja:$PATH" \
    third-party/make/make -j"$n" --jobserver-style=fifo -f "$mk" all
else:
  exec make -j"$n" NROS_JOBSERVER=0 NROS_BUILD_LOG_DIR="$log_dir" -f "$mk" all
```

Generated makefile shape:

```make
SHELL := /usr/bin/env bash
.DELETE_ON_ERROR:

LOG_DIR := /abs/tmp/fixture-make-all-...
JOBLOG := $(LOG_DIR)/fixtures.joblog

.PHONY: all preflight stamp <fixture-leaves>

all: preflight <fixture-leaves> stamp

preflight:
	+@mkdir -p "$(LOG_DIR)"
	+@printf 'stage\tstart_epoch\tend_epoch\tduration_seconds\tstatus\tlog\n' > "$(JOBLOG)"

stamp:
	+@mkdir -p target/nextest
	+@date -u +%Y-%m-%dT%H:%M:%SZ > target/nextest/.fixtures-built
```

Leaf names:

- `fixture/<platform>/rust/<role>/<rmw-or-default>`;
- `fixture/<platform>/c/<role>/<rmw>`;
- `fixture/<platform>/cpp/<role>/<rmw>`;
- `fixture/zephyr/<board>/<lang>/<role>/<rmw>`;
- `fixture/<platform>/cargo/<sanitized-path>` for ad-hoc bins;
- `preflight/<platform>/<name>` for serial prerequisites.

Each leaf writes `LOG_DIR/<target>.log`, appends one joblog row, and
tails the failed log. Direct platform recipes eventually become thin
wrappers around `./scripts/build/fixture-make-driver.sh <platform>`,
with only platform env preserved.

### Parallel Wave 2 — First Implementation Split

Started 2026-06-07. Scope is small implementation slices plus two
read-only design/audit tasks. Behavior changes stayed focused and
independently reviewable.

- [x] Native Cyclone manifest cleanup: add native C/C++ Cyclone rows,
      route them through `fixtures-build.sh`, and stop deleting
      `build-cyclonedds`.
- [x] Fixture make-driver skeleton: add an unwired diagnostic/skeleton
      driver for native manifest-driven fixture groups with fifo make
      and ordinary make fallback.
- [x] C/C++ efficiency measurement runner: add an opt-in script to
      collect sccache stats and parse real rebuild signals.
- [x] Manifest coverage cleanup audit: produce the patch plan for
      STM32F4, QEMU bare-metal, NuttX, ESP32, and logging-smoke gaps.
- [x] Zephyr leaf generator design: define Zephyr fixture leaf records
      and preflights for future make-driver integration.

Wave 2 result: the first implementation slice landed in the working
tree: native C/C++ Cyclone rows now live in `examples/fixtures.toml`,
and `just/native.just` routes them through `fixtures-build.sh` without
deleting `build-cyclonedds`. Two opt-in support scripts also exist:

- `scripts/build/fixture-make-driver.sh` — unwired skeleton driver for
  native manifest-driven fixture groups. It generates a temporary
  makefile under `build/fixture-make-driver/`, prefers pinned fifo make
  when available, falls back to ordinary make, and supports `--dry-run`.
- `scripts/build/phase226-cxx-eff.sh` — diagnostic C/C++ efficiency
  runner that writes logs under `tmp/phase226-cxx-eff/`, captures
  sccache stats, and summarizes real `nros-c` / `nros-cpp` compile
  lines, C/CXX object builds, links, and Cargo fingerprint lines.

Validation performed in Wave 2:

- `python3 scripts/build/fixtures-manifest.py list --platform native --lang c --rmw cyclonedds`
- `python3 scripts/build/fixtures-manifest.py list --platform native --lang cpp --rmw cyclonedds`
- `bash -n scripts/build/fixture-make-driver.sh`
- `scripts/build/fixture-make-driver.sh --dry-run native`
- `bash -n scripts/build/phase226-cxx-eff.sh`
- `scripts/build/phase226-cxx-eff.sh --help`
- `scripts/build/phase226-cxx-eff.sh --lang c --rmw cyclonedds --role talker --dry-run`

No full fixture build was run in this wave.

### Parallel Wave 3 — Manifest Coverage and Leaf Records

Started 2026-06-07. Scope was the next low-conflict implementation
slice from the Wave 2 plan plus focused measurement. NuttX was held for
the next wave because it also edits `examples/fixtures.toml`.

- [x] STM32F4 manifest routing: replace the hard-coded Rust fixture
      loop in `just/stm32f4.just` with `fixtures-build.sh stm32f4
      rust`, preserving the `arm-none-eabi-gcc` guard and the previous
      explicit `thumbv7em-none-eabihf` target.
- [x] QEMU bare-metal manifest coverage: add the direct Rust fixture
      leaves to `examples/fixtures.toml` and route
      `just/qemu-baremetal.just` through `fixtures-build.sh
      qemu-arm-baremetal rust`.
- [x] Fixture make-driver hardening: add joblog/status files, failed log
      tailing, and richer dry-run leaf output to
      `scripts/build/fixture-make-driver.sh`.
- [x] Zephyr leaf record prototype: add
      `scripts/build/zephyr-fixture-leaves.sh` to emit tab-separated
      Zephyr fixture records without changing the current Zephyr build
      path.
- [x] Focused native Cyclone C/C++ measurement: run the diagnostic
      script for one C and one C++ Cyclone talker cell.

Wave 3 result: STM32F4 and QEMU bare-metal now use the fixture manifest
for the selected direct fixture lists. QEMU keeps
`packages/reference/qemu-smoltcp-bridge` as an ad-hoc build until
reference package coverage moves into the manifest. The make-driver
remains unwired, but now has the accounting needed to compare leaf
runtime and diagnose failures. The Zephyr helper is intentionally
record-only; it does not run `west`, `ninja`, Cargo, or CMake.

Focused C/C++ measurement:

- C Cyclone talker cell:
  `scripts/build/phase226-cxx-eff.sh --lang c --rmw cyclonedds --role talker --limit 1`
  wrote logs under `tmp/phase226-cxx-eff/20260607-113200` and reported
  `Compiling nros-c: 1`, `Compiling nros-cpp: 1`, `C object builds:
  119`, `CXX object builds: 5`, `link steps: 3`, and `cargo fingerprint
  lines: 538`.
- C++ Cyclone talker cell:
  `scripts/build/phase226-cxx-eff.sh --lang cpp --rmw cyclonedds --role talker --limit 1`
  wrote logs under `tmp/phase226-cxx-eff/20260607-113504` and reported
  `Compiling nros-c: 1`, `Compiling nros-cpp: 1`, `C object builds:
  150`, `CXX object builds: 6`, `link steps: 2`, and `cargo fingerprint
  lines: 570`.

Validation performed in Wave 3:

- `bash -n scripts/build/fixture-make-driver.sh scripts/build/zephyr-fixture-leaves.sh scripts/build/phase226-cxx-eff.sh`
- `scripts/build/fixture-make-driver.sh --dry-run native`
- `scripts/build/zephyr-fixture-leaves.sh --emit records --filter 'build-rs-talker-zenoh|build-c-talker-xrce'`
- `python3 scripts/build/fixtures-manifest.py list --platform stm32f4 --lang rust`
- `python3 scripts/build/fixtures-manifest.py list --platform qemu-arm-baremetal --lang rust`
- `scripts/build/phase226-cxx-eff.sh --lang c --rmw cyclonedds --role talker --dry-run`
- `scripts/build/phase226-cxx-eff.sh --lang c --rmw cyclonedds --role talker --limit 1`
- `scripts/build/phase226-cxx-eff.sh --lang cpp --rmw cyclonedds --role talker --limit 1`

Post-wave validation:

- `XDG_RUNTIME_DIR=/tmp just setup-cli` built the in-tree CLI at
  `packages/cli/target/release/nros`. The user environment still has a
  stale `/home/aeon/.nros/bin/nros` earlier in `PATH`, so platform
  validation pinned `NROS_CLI` to the in-tree binary.
- `XDG_RUNTIME_DIR=/tmp NROS_CLI=$PWD/packages/cli/target/release/nros just qemu build-fixtures`
  passed. It exercised the manifest-routed QEMU rows, including
  `talker-xrce`, `phase216-rtic-e2e`, the test bins, the bench bins,
  and the ad-hoc `packages/reference/qemu-smoltcp-bridge` build.
- `XDG_RUNTIME_DIR=/tmp NROS_CLI=$PWD/packages/cli/target/release/nros just stm32f4 build-fixtures`
  reached real compilation and failed at link time on existing RTIC rows
  with unresolved `_defmt_timestamp`. An initial run also showed that
  the STM32F4 manifest rows lacked the old recipe's explicit
  `--target thumbv7em-none-eabihf`; the manifest rows were updated to
  carry that target, and the rerun confirmed the target is now passed.

### Wave 4 Investigation — Make-Driver Wiring

Started 2026-06-07. Scope was investigation only: identify the next safe
step for wiring `scripts/build/fixture-make-driver.sh` beyond native
dry-run, especially native make leaves and Zephyr leaf records.

Observed driver state:

- `scripts/build/fixture-make-driver.sh` is still hard-gated to
  `native|all` and emits native manifest groups only. A dry-run currently
  emits nine grouped leaves: native Rust default, native Rust zenoh/xrce,
  native C zenoh/xrce/cyclonedds, and native C++ zenoh/xrce/cyclonedds.
- Native leaves call `scripts/build/fixtures-build.sh native <lang>
  [rmw]`. That makes the driver safe as a replacement for the existing
  sequential C/C++ manifest passes, but it is not yet a per-fixture-row
  scheduler and therefore will not solve Rust target-dir sharing by
  itself.
- The current native `just` path still has two raw background Cargo leaves
  for pure-Cargo Cyclone Rust talker/listener before the manifest C/C++
  passes. Those should become explicit make leaves before the driver
  replaces `just native build-fixture-extras`.

Observed Zephyr state:

- `scripts/build/zephyr-fixture-leaves.sh` now emits stable identity and
  config records for the Zephyr matrix plus optional logging-smoke, but
  the executable decision is still incomplete.
- The record's `needs_west` field is `unknown`. Current
  `just/zephyr-ci.just` still owns the signature comparison,
  cached-`MAKE` validation, Cyclone stale-source clean-reconfigure
  guard, and `ninja` versus `west build` choice.
- The record's `argv_ninja` and `argv_west` fields are diagnostic strings,
  not shell-escaped argv arrays. A make-driver implementation should not
  `eval` those strings; it should either compute the command from parsed
  fields or move the current Zephyr build-one logic into a dedicated leaf
  runner script.

Recommended next implementation step:

1. Add a small native leaf-runner mode to
   `scripts/build/fixture-make-driver.sh` for the two pure-Cargo Cyclone
   Rust leaves, preserving the existing `nros ws sync` preflight and
   `target-cyclonedds/` target dir from `just/native.just`.
2. Run the make-driver for native only, without wiring `just/native.just`.
   Validate that the joblog/status output and failure tailing work on a
   real build.
3. After that, wire only `just native build-fixture-extras` to the
   driver. Leave Zephyr scheduling unchanged until a Zephyr leaf-runner
   script can own the `needs_west` decision without duplicating or
   weakening the current checks.

Risks to manage:

- Native C/C++ grouped leaves may run concurrently across RMWs and langs;
  confirm their build dirs and `NROS_CMAKE_EXTRA_DEFS` are isolated before
  enabling default parallel execution.
- The native pure-Cargo Cyclone leaves share example directories with
  other native Rust rows. Keep `target-cyclonedds/` isolated and keep
  codegen preflight serial.
- Zephyr direct make leaves are higher risk than native leaves because the
  current recipe has workspace patching, generated-dir sync, venv/PATH
  setup, ccache dirs, signature checks, and stale Cyclone handling in one
  shell body. Move this into a leaf runner before changing scheduling.

### Parallel Wave 4 — Manifest Selectors and RTOS Extras

Started 2026-06-07. Scope was medium-risk manifest coverage plus the
selector support needed to move smoke-only rows without broadening normal
platform builds.

- [x] NuttX Rust manifest routing: add
      `packages/testing/nros-tests/bins/logging-smoke-nuttx-qemu-arm` to
      `examples/fixtures.toml` and replace the manual Rust fixture loop
      in `just/nuttx.just` with `fixtures-build.sh nuttx rust`. The
      existing kernel provisioning, rustup warm-up, and release-profile
      workaround remain in the recipe.
- [x] QEMU ESP32 manifest coverage: add
      `packages/testing/nros-tests/bins/logging-smoke-esp32-qemu` to the
      `qemu-esp32-baremetal` fixture rows with `skip_probe = true`.
- [x] Fixture `--id` selection: add optional `--id` filtering to
      `fixtures-manifest.py list`, `fixtures-manifest.py
      list-workspaces`, and `fixtures-build.sh`.
- [x] ESP32 targeted builds: add stable IDs for the QEMU ESP32 talker,
      listener, and logging-smoke rows. `just/esp32.just` now builds the
      talker/listener rows by ID for QEMU image packaging and builds the
      logging-smoke row by ID before smoke image packaging.

Validation performed in Wave 4:

- `bash -n scripts/build/fixtures-build.sh`
- `python3 -m py_compile scripts/build/fixtures-manifest.py`
- `python3 scripts/build/fixtures-manifest.py list --platform qemu-esp32-baremetal --lang rust`
- `python3 scripts/build/fixtures-manifest.py list --platform qemu-esp32-baremetal --lang rust --id qemu-esp32-baremetal-logging-smoke`
- `python3 scripts/build/fixtures-manifest.py list --platform qemu-esp32-baremetal --lang rust --for-probe | wc -l`
- `python3 scripts/build/fixtures-manifest.py list --platform nuttx --lang rust`
- `scripts/build/fixtures-build.sh native rust --id no-such-fixture`
- `scripts/build/fixtures-build.sh native c zenoh --id no-such-fixture`
- `just --list --justfile just/esp32.just`
- `just --list --justfile just/nuttx.just`

No full NuttX or ESP32 fixture build was run in this wave.

### Parallel Wave 5 — Native Rust Cyclone Make Driver

Started 2026-06-07. Scope was the low-risk native make-driver wiring
identified by the Wave 4 investigation.

- [x] FreeRTOS and ThreadX smoke rows: add stable manifest IDs for
      FreeRTOS MPS2, ThreadX Linux, and ThreadX RISC-V logging-smoke
      fixtures, then build those smoke rows through `fixtures-build.sh
      --id`.
- [x] Native pure-Cargo Cyclone Rust leaves: add a
      `native-cyclonedds-rust` make-driver scope for the Rust talker and
      listener, keeping the recipe-owned `nros ws sync` preflight and
      `target-cyclonedds/` target dirs.
- [x] Wire `just native build-fixture-extras` to run the native Cyclone
      Rust leaves through `scripts/build/fixture-make-driver.sh` before
      the existing native C/C++ fixture passes.

Validation performed in Wave 5:

- `bash -n scripts/build/fixture-make-driver.sh scripts/build/fixtures-build.sh`
- `scripts/build/fixture-make-driver.sh --dry-run native-cyclonedds-rust`
- `scripts/build/fixture-make-driver.sh --dry-run native`
- `scripts/build/fixture-make-driver.sh native-cyclonedds-rust`
  intentionally failed when run directly against stale generated message
  code, validating failure tails, status files, and joblog emission.
- `XDG_RUNTIME_DIR=/tmp NROS_CLI=/home/aeon/repos/nano-ros/packages/cli/target/release/nros just native build-fixture-extras`
  passed. The recipe preflight regenerated stale native Rust message
  code first, then the make-driver built both native Cyclone Rust leaves
  successfully:
  - `fixture-native-rust-cyclonedds-talker`: ok, 39 s
  - `fixture-native-rust-cyclonedds-listener`: ok, 39 s

Observed follow-up during validation:

- The remaining slow path is the existing native C/C++ fixture tail.
  It still launches separate CMake/Corrosion build trees for Zenoh,
  XRCE, and CycloneDDS examples. The run repeatedly compiled shared
  Rust crates, `nros-c`, `nros-cpp`, C++ FFI glue, and CycloneDDS/type
  support in per-example build dirs.
- Follow-up inspection found no active unconditional native
  `rm -rf build-cyclonedds` in normal fixture builds. Build dirs are
  removed by native clean recipes and by helper generator-switch cleanup
  only. The real warm-build issue is isolated per-example CMake and
  Corrosion state plus C/C++ manifest cells still using GNU parallel.
- The next implementation slice should focus on native C/C++ Cyclone
  make-driver routing and measurement before any shared Corrosion target
  directory change.

Wave 6 candidate from this point:

1. Add a `native-cyclonedds-cmake` make-driver scope for the native
   Cyclone C/C++ manifest rows, implemented first as grouped C and C++
   leaves. Keep per-example CMake build dirs isolated.
2. Route only `just native build-fixture-extras` Cyclone C/C++ passes
   through that scope. Keep Zenoh/XRCE unchanged initially.
3. Capture focused before/after metrics with
   `scripts/build/phase226-cxx-eff.sh` for one C cell, one C++ cell, and
   the full native Cyclone C/C++ matrix. Count real `Compiling nros-c`,
   `Compiling nros-cpp`, object builds, link steps, and sccache stats.
4. Leave Zephyr scheduling out of this slice. It still needs a
   Zephyr-specific leaf runner that preserves the parent preflight,
   signature/cache decision, `west build` versus `ninja -C` selection,
   nested job budgeting, and logging-smoke handling.

### Parallel Wave 6 — Native Cyclone C/C++ Make Driver

Started 2026-06-07. Scope was the next low-risk native C/C++ slice
identified by Wave 5.

- [x] Add a `native-cyclonedds-cmake` make-driver scope.
- [x] Generate two grouped make leaves:
      `fixture-native-c-cyclonedds` and
      `fixture-native-cpp-cyclonedds`.
- [x] Run each grouped leaf with `NROS_JOBSERVER=1
      scripts/build/fixtures-build.sh native <c|cpp> cyclonedds`, so the
      inner manifest builder serializes its rows instead of invoking GNU
      parallel.
- [x] Route only the Cyclone C/C++ tail of
      `just native build-fixture-extras` through the new make-driver
      scope. Zenoh and XRCE C/C++ passes remain unchanged.

Validation performed in Wave 6:

- `bash -n scripts/build/fixture-make-driver.sh scripts/build/fixtures-build.sh`
- `scripts/build/fixture-make-driver.sh --dry-run native-cyclonedds-cmake`
- `just --list --justfile just/native.just`
- Direct focused run with the same Cyclone CMake definitions used by
  `just native build-fixture-extras`:
  - `fixture-native-c-cyclonedds`: ok, 14 s
  - `fixture-native-cpp-cyclonedds`: ok, 2 s
- `XDG_RUNTIME_DIR=/tmp NROS_CLI=/home/aeon/repos/nano-ros/packages/cli/target/release/nros NROS_BUILD_JOBS=8 just native build-fixture-extras`
  passed. The native Cyclone C/C++ make-driver joblog recorded both
  grouped leaves as successful:
  - `fixture-native-c-cyclonedds`: ok, 7 s
  - `fixture-native-cpp-cyclonedds`: ok, 7 s

Remaining native C/C++ work:

- Measure the per-example CMake/Corrosion warm-build behavior before any
  shared target-dir change. The current wave deliberately kept isolated
  `build-${rmw}` directories.
- Decide whether Zenoh and XRCE C/C++ fixture passes should also route
  through the make-driver after the Cyclone path remains stable. Done in
  Wave 7.
- Keep Zephyr scheduling out of the native cleanup. Zephyr still needs a
  dedicated leaf runner that preserves its preflight and `west build`
  versus `ninja -C` boundary.

### Parallel Wave 7 — Native CMake RMW Groups and No GNU Parallel

Started 2026-06-07. Scope was the remaining native C/C++ fixture
orchestration and the shared manifest builder's GNU parallel fallback.

- [x] Add a `native-cmake-rmw` make-driver scope for the native Zenoh
      and XRCE C/C++ manifest groups.
- [x] Route the native Zenoh/XRCE C/C++ tail of
      `just native build-fixture-extras` through the new make-driver
      scope. Cyclone C/C++ remains in its own scope because it needs the
      Cyclone IDL/codegen CMake definitions.
- [x] Replace `scripts/build/fixtures-build.sh` GNU parallel fallback
      with a temporary makefile fallback. When `NROS_JOBSERVER=1` is
      set, it still runs serially so nested tools inherit the outer fifo
      jobserver.
- [x] Add random suffixes to generated make-driver and manifest-builder
      makefile names so simultaneous dry-runs/builds cannot collide on
      the same timestamp.

Validation performed in Wave 7:

- `bash -n scripts/build/fixtures-build.sh`
- `bash -n scripts/build/fixture-make-driver.sh`
- `scripts/build/fixture-make-driver.sh --dry-run native-cmake-rmw`
- `scripts/build/fixture-make-driver.sh --dry-run native-cyclonedds-cmake`
- `rg -n "parallel --jobs|command -v parallel|\bparallel\b" scripts/build/fixtures-build.sh scripts/build/fixture-make-driver.sh just/native.just`
  now finds no executable GNU parallel call in fixture scheduling paths.
  Remaining `just/native.just` matches are non-fixture example/check
  recipes.
- Direct manifest-builder fallback:
  `NROS_BUILD_JOBS=4 scripts/build/fixtures-build.sh native c zenoh`
  passed after fixing generated makefile quoting for unit-separator
  fixture records. Ninja reported fifo jobserver mode.
- `XDG_RUNTIME_DIR=/tmp NROS_CLI=/home/aeon/repos/nano-ros/packages/cli/target/release/nros NROS_BUILD_JOBS=8 just native build-fixture-extras`
  passed. Warm joblog timings:
  - `fixture-native-c-zenoh`: ok, 2 s
  - `fixture-native-c-xrce`: ok, 2 s
  - `fixture-native-cpp-zenoh`: ok, 3 s
  - `fixture-native-cpp-xrce`: ok, 2 s
  - `fixture-native-c-cyclonedds`: ok, 1 s
  - `fixture-native-cpp-cyclonedds`: ok, 1 s

Remaining implementation work:

- Zephyr still owns shell-array background scheduling. The safe migration
  needs a Zephyr one-leaf runner that preserves the existing preflight,
  signature/cache decision, `west build` versus `ninja -C` selection,
  nested job budgeting, logging-smoke handling, and logs under
  `build/zephyr-fixtures`.
- Explicit nested job flags remain in platform recipes for non-jobserver
  fallback and in Zephyr's current scheduler.

### Wave 2 Findings — Manifest Coverage Cleanup Plan

Recommended follow-up order:

1. **STM32F4, low risk — done in Wave 3.** Replace the hard-coded list in
   `just/stm32f4.just:42` with
   `bash scripts/build/fixtures-build.sh stm32f4 rust`. The manifest
   already has the direct-list gap, `talker-embassy`
   (`examples/fixtures.toml:457`). Keep the `arm-none-eabi-gcc` guard.
2. **QEMU bare-metal, low/medium risk — done in Wave 3.** Add direct leaves to the QEMU
   manifest section: `talker-xrce`, `phase216-rtic-e2e`,
   `cdr-roundtrip-qemu`, `lan9118-qemu`,
   `logging-smoke-mps2-baremetal`, `wcet-cycles-qemu`, and
   `large-msg-baremetal`. Then replace the `find` plus manual list in
   `just/qemu-baremetal.just:129` with
   `fixtures-build.sh qemu-arm-baremetal rust`. Do not manifest
   `phase216_rtic_e2e_pkg`; it is a dependency package.
3. **NuttX Rust, medium risk — done in Wave 4.** Add
   `packages/testing/nros-tests/bins/logging-smoke-nuttx-qemu-arm`
   with `env = { CC_armv7a_nuttx_eabi = "arm-none-eabi-gcc" }`, then
   replace the manual Rust loop in `just/nuttx.just:130` with
   `NROS_CARGO_PROFILE=release fixtures-build.sh nuttx rust`. Preserve
   the kernel build and rustup warm-up preflights.
4. **ESP32 Cargo leaves, medium risk — done in Wave 4.** The qemu ESP32 talker/listener
   rows already exist. Add `logging-smoke-esp32-qemu` as a
   `qemu-esp32-baremetal` Rust row with `skip_probe = true`; keep
   `espflash save-image` as ad-hoc image packaging.
5. **Smoke-only extras, higher-risk prerequisite — selector done in
   Wave 4.** Use `--id` support to move FreeRTOS, ThreadX Linux, and
   ThreadX RV64 logging-smoke extras into the manifest. Zephyr
   logging-smoke should wait for Zephyr leaf generation because it is a
   west/CMake image build, not a Cargo leaf.

### Wave 2 Findings — Zephyr Leaf Generator Plan

Add a shell generator such as `scripts/build/zephyr-fixture-leaves.sh`
beside `scripts/build/fixture-matrix.sh`. First slice should be
record-only: keep current Zephyr scheduling unchanged, but have
`just/zephyr-ci.just` consume generated records into its existing
`entries` array. Later, the make driver consumes the same records.

Proposed record fields:

- identity: `kind`, `id`, `target`, `board`, `lang`, `lang_tag`,
  `role`, `rmw`;
- paths: `src`, `src_dir`, `build_dir`, `build_name`, `log`;
- transport config: `xrce_agent_port`, `zenoh_locator`,
  `cyclone_domain`, `conf_files`;
- build config: `extra_cmake_defs`, `sig`, `sig_file`,
  `best_effort`;
- optional future make fields: `command_mode`, `needs_west`,
  `eff_pristine`, `argv_ninja`, `argv_west`, and env fields.

Generation must preserve current formulas exactly:

- Zephyr 4.4 emits only zenoh; 3.7 emits zenoh/xrce and conditional
  Cyclone (`just/zephyr-ci.just:222`).
- Roles/langs come from `scripts/build/fixture-matrix.sh`.
- XRCE ports, Zenoh locators, Cyclone domains, `CONF_FILE`, and the
  signature input must remain byte-for-byte equivalent to current logic.

Preflights remain serial: workspace validation, ROS interface defaults,
Zephyr venv/toolchain env, patching, build/log/cache dirs, Rust
`nros ws sync`, host codegen tool, and job/pristine/sccache validation.

### 226.A — Inventory the Fixture Graph

- [ ] Generate a complete fixture leaf list from `examples/fixtures.toml`
      plus hand-authored platform leaves.
- [ ] Classify each leaf as Cargo, CMake, Zephyr, workspace, preflight,
      smoke image packaging, or external SDK provisioning.
- [ ] Identify leaves that currently mutate shared directories and must
      run as serialized prerequisites.

Acceptance:

- The generated inventory covers everything `just build-test-fixtures`
  and `just <platform> build-fixtures` currently builds.
- No fixture leaf is still discovered only by ad-hoc `find` in a recipe.

### 226.B — Introduce a Fixture Make Driver

- [x] Add a script that emits or invokes a makefile for one platform or
      for the full fixture matrix.
- [x] Use pinned GNU make 4.4 fifo mode when available.
- [x] Use ordinary make `-j` fallback when pinned make/ninja are absent.
- [x] Keep logging/joblog behavior equivalent or better than today's
      `tmp/build-test-fixtures-latest`.

Acceptance:

- `just <platform> build-fixtures` has no GNU parallel dependency.
- `just build-test-fixtures` has no GNU parallel dependency.
- Build failure reporting still names the failed fixture leaf and points
  to a useful log.

### 226.C — Remove Hidden Fan-Out

- [x] Replace native pure-Cargo Cyclone Rust raw `&` / `wait` loops with
      make leaves.
- [x] Replace native Cyclone C/C++ fixture fan-out with grouped make
      leaves for the Cyclone C and C++ manifest passes.
- [ ] Replace Zephyr shell-array background scheduling with make leaves.
- [x] Remove fixture-path GNU parallel calls.
- [ ] Remove explicit Ninja/CMake/Cargo job flags from jobserver leaves.

Acceptance:

- `rg 'parallel --jobs|\\) &|CMAKE_BUILD_PARALLEL_LEVEL|ninja -C .* -j'`
  has no matches in fixture scheduling paths, except documented
  non-fixture commands or deliberate pure-make fallback code.

### 226.D — Shared Rust Fixture Target Dirs

- [ ] Add grouping logic for compatible Rust fixture rows.
- [ ] Apply shared fixture-only `--target-dir` to qemu bare-metal,
      stm32f4, ESP32/QEMU-ESP32, and compatible native/RTOS rows.
- [ ] Keep feature/RMW/env variants isolated where sharing would cause
      feature thrash or stale artifacts.

Acceptance:

- Repeated `Compiling nros-c` / shared nano-ros crate rebuilds are
  materially reduced for same-platform same-feature fixture groups.
- Manual `cargo build` inside an example still uses the example-local
  standalone target dir.

### 226.E — CMake/Corrosion Efficiency Audit

- [ ] Audit per-example CMake build dirs that create separate Corrosion
      Cargo target trees.
- [ ] Capture sccache stats around native, Zephyr, FreeRTOS, ThreadX,
      and representative Cyclone C/C++ fixture builds.
- [ ] Count real `nros-c` / `nros-cpp` recompiles from build output,
      distinguishing Cargo fingerprint checks from actual `Compiling`
      lines.
- [ ] Identify whether misses are caused by target triple, platform
      feature set, env/toolchain differences, clean build dirs, or
      scheduler fan-out.
- [ ] Remove native Cyclone `rm -rf build-cyclonedds` from normal
      fixture builds.

Acceptance:

- Warm `just native build-fixtures` does not force clean Cyclone C/C++
  rebuilds.
- The phase has measured before/after C/C++ fixture data before any
  shared Corrosion target-dir change is attempted.
- Any proposed cache sharing is limited to same target triple,
  same platform feature set, same profile, and same relevant env.

### 226.F — Validation

- [ ] Capture before/after timings for representative direct platform
      fixture builds: native, qemu, zephyr, freertos, nuttx.
- [ ] Capture before/after `just build-test-fixtures` timing.
- [ ] Check CPU utilization under `NROS_BUILD_JOBS=8` and a high-core
      default run.
- [ ] Verify full runtime suite still consumes the same fixture paths.

Acceptance:

- `just build-test-fixtures` exits 0 after a clean build.
- `just test-all` does not report stale or missing fixture binaries due
  to changed target/build dirs.
- Direct `just <platform> build-fixtures` remains supported and does not
  require GNU parallel.

---

## 8. Non-Goals

- Do not change the user-facing standalone example layout.
- Do not require a full build-system rewrite to Bazel/Buck/Ninja.
- Do not merge semantically distinct fixture variants into one runtime
  binary just to reduce compile count.
- Do not make `nros` replace Cargo, CMake, West, or board build tools.

---

## 9. Initial File Map

Primary files to modify later:

- `justfile`
- `build-all.mk`
- `scripts/build-all-jobserver.sh`
- `scripts/build/fixtures-build.sh`
- `scripts/build/workspace-fixtures-build.sh`
- `scripts/build/cargo.sh`
- `just/native.just`
- `just/qemu-baremetal.just`
- `just/stm32f4.just`
- `just/freertos.just`
- `just/nuttx.just`
- `just/threadx-linux.just`
- `just/threadx-riscv64.just`
- `just/zephyr-ci.just`
- `just/esp32.just`
- `examples/fixtures.toml`

Prior phases with relevant context:

- `docs/roadmap/archived/phase-176-unified-jobserver-build-orchestration.md`
- `docs/roadmap/archived/phase-178-build-system-dedup-and-fixture-cache.md`
- `docs/roadmap/archived/phase-181-fixture-build-ssot.md`
- `docs/roadmap/phase-225-workspace-fixture-migration.md`
