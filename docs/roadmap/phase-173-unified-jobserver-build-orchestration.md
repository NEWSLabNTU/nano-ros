# Phase 173 - Unified jobserver build orchestration

**Goal.** Replace the static, per-platform parallelism split in
`build-all` with a single GNU make jobserver shared across every build
stage (cargo, build-script `cc`, ninja-via-west, cmake), so the whole
build draws from one dynamically-allocated token pool. The long pole
(zephyr) soaks up tokens freed by finished platforms instead of idling
on a fixed 1/Nth share.

**Status.** **Landed + validated end-to-end (A/B/C/D).** Pinned make
4.4.1 + ninja 1.13.2 install (173.A); `build-all.mk` + `just
build-all-jobserver` (173.B); all downstream `-j` stripped — zephyr
`CMAKE_BUILD_PARALLEL_LEVEL`, cmake `--parallel` in the C/C++ recipes,
+ a `gmake`→make-4.4 alias so stray sub-makes don't choke on the fifo
auth (173.C). **Full-sweep validation (173.D)**: `NROS_BUILD_JOBS=32
just build-all-jobserver` ran the whole build under one
`make 4.4 -j32 --jobserver-style=fifo` pool — cargo rustc throttles to
the pool, ninja 1.13 logged `Jobserver mode detected: fifo:…` in 55
zephyr builds, 0 sub-make fifo errors. The only failure was the
pre-existing cyclonedds-zephyr `nsos_adapt.c` duplicate-case patch bug
(Phase 171.0.a, unrelated) — every other stage built clean under the
shared pool. Supersedes the static `NROS_BUILD_JOBS` outer×inner split
for `build-all-jobserver`; plain `build-all` keeps the static split as
the no-extra-toolchain fallback.

**Priority.** P3 (perf/ergonomics). The current static split
(`NROS_BUILD_JOBS` budget, `build-test-fixtures` pool + zephyr solo
full-budget track, zephyr `BUILD_JOBS × ninja`) is "good enough" and
shipping; this phase is the proper fix for residual tail
under-utilization.

**Depends on.**

- **ninja ≥ 1.13** — GNU jobserver *client* support landed in ninja
  1.13 (2025). The system ninja is 1.10.1 (no jobserver). Must
  build/pin a newer ninja (mirrors the existing patched-`qemu-system-arm`
  build-from-source pattern under `third-party/qemu/`).
- **GNU make ≥ 4.4** — needs `--jobserver-style=fifo`; the fifo
  jobserver survives the `cargo → build-script → cc` and
  `west → cmake → ninja` process chains, where make 4.3's
  pipe-fd jobserver can break when a tool closes inherited fds. Host
  has make 4.3.
- cargo (1.95 present) already a jobserver client; `cc` crate
  (build scripts) already jobserver-aware. No change needed there.

## Background — why the static split has a tail

`build-all` is heterogeneous: cargo (7 platforms), ninja-via-west
(zephyr), cmake (C/C++ examples), GNU `parallel` orchestration. No
single scheduler spans them, so `just/justfile` pre-splits the budget
(`outer` platforms × `inner` jobs; zephyr `BUILD_JOBS × ninja`). The
split is fixed at launch, so when the fast platforms (qemu ~5s,
native ~60s) finish, their share is stranded while zephyr (~1000s of
per-example picolibc + kernel builds) keeps only its slice. A jobserver
reallocates freed tokens dynamically → no stranded cores.

## Target design — one make jobserver, every stage a client

### Provider

`make -jN --jobserver-style=fifo` at the top is the token server +
scheduler. It exports the fifo path via `MAKEFLAGS`; all descendants
inherit it. `N` is the single `NROS_BUILD_JOBS` knob (defaults to
nproc).

### Stage wiring

- **cargo** — reads `MAKEFLAGS`, detects the inherited jobserver,
  throttles rustc + build-script jobs against the shared pool. Invoke
  **without** `-j` / `CARGO_BUILD_JOBS` (those force cargo's own pool and
  ignore the parent).
- **build-script `cc`** — `cc::Build` (zenoh-pico, nros-c `log_fmt`,
  weak stubs) is jobserver-aware; its gcc invocations join the pool
  automatically once cargo is under the jobserver.
- **ninja-via-west** (ninja ≥1.13) — consumes the jobserver when
  `MAKEFLAGS` carries it **and no explicit `-j`**. The west invocation
  must **drop** `CMAKE_BUILD_PARALLEL_LEVEL` / the `NINJA_JOBS` `-j`
  (passing `-jN` overrides the jobserver). Inherit only.
- **cmake --build** — shells the generator (ninja or sub-make), both of
  which inherit the jobserver. Must **not** pass `--parallel N`.

### Orchestration — make replaces GNU parallel

Emit a generated `build-all.mk` with **one independent target per build
step** (each platform's cargo builds, each zephyr example west build,
each cmake C/C++ example). `make -jN -f build-all.mk` schedules them:
each running recipe gets 1 implicit token and its tool draws more from
the fifo. This drops the second, independent `parallel --jobs` throttle
— the jobserver becomes the only one.

(Keeping `parallel --jobs 0` as a pure launcher is possible but
parallel-spawned procs sit outside make's implicit-token accounting and
over-count by ~1 per proc; make-as-scheduler is cleaner.)

## Work items

### 173.A — toolchain

- [ ] Build/pin ninja ≥1.13 (build-from-source under `third-party/`,
      mirror the patched-qemu recipe); test-harness/recipe path picks it
      up like `nros_tests::qemu::qemu_system_arm_path()`.
- [ ] Build/pin make ≥4.4 (or document the apt source) for fifo
      jobserver.
- [ ] `just doctor` checks for ninja ≥1.13 + make ≥4.4; falls back to
      the static split when absent.

### 173.B — `build-all.mk` generator

- [ ] Generate independent targets for every build step from the same
      platform/example lists the current recipes walk.
- [ ] `make -jN --jobserver-style=fifo -f build-all.mk` entry point;
      `NROS_BUILD_JOBS` → `N`.

### 173.C — strip downstream job flags

- [ ] Remove every explicit `-j` / `--parallel N` /
      `CMAKE_BUILD_PARALLEL_LEVEL` / cargo `-j` from the per-stage
      invocations so each stage inherits the jobserver instead of
      detaching from it. (Audit `just/*.just` — these were *added* by
      the Phase 165.perf static-split work; this phase removes them.)
- [ ] Verify `MAKEFLAGS` propagates unmodified through `just` → recipe →
      tool (don't sanitize the env).

### 173.D — validation

- [ ] `htop` shows sustained ~N utilization through the whole build,
      including the zephyr tail (no idle cores once fast platforms
      finish).
- [ ] `NROS_BUILD_JOBS=8 just build-all` caps total concurrency at 8
      across all stages simultaneously.
- [ ] Build output identical to the static-split path.

## Notes

- Hard rule: **any** explicit job flag on **any** stage detaches it from
  the pool — the audit in 173.C is the load-bearing part.
- The fifo jobserver path must survive `cargo → build.rs → cc` and
  `west → cmake → ninja`; that's exactly why make 4.4 fifo (not 4.3
  pipe) is required.
- Bazel/Buck2 would give the same dynamic allocation via a real build
  graph, but that's a full build-system rewrite — explicitly out of
  scope; this phase keeps just/cargo/cmake/west and only unifies their
  parallelism.
- Supersedes the static `NROS_BUILD_JOBS` outer×inner split (Phase
  165.perf in `justfile` / `just/*.just`) once landed; until then the
  static split stays the default.
