# Phase 240 — CI disk + build-time optimization

**Goal.** Stop the per-platform CI lanes from failing on `No space left on device`
(the ~14 GB GitHub container runners can't hold the full SDK + cargo + ROS build
for the heavy cells), and cut per-push wall-clock — **without** losing the
on-demand full end-to-end (e2e) validation that complete coverage needs. Three
disk levers (maintainer directive): pull/build only what a run needs, shallow
clone everywhere, reclaim space once a binary is built.

**Status.** Near-complete (2026-06-12). The build-scope + shallow + reclaim levers
landed and are **validated** by dispatch run 27393704883: the heavy embedded cells
no longer die on `No space left on device` or the embedded compile — nuttx ran
~10 min into the cpp fixtures, esp32 went fully green. The one remaining e2e red
(nuttx cpp `div_t` header clash) is a pre-existing fixture bug, not a disk/build
issue — handed to issue #34's cpp bucket. Several CI regressions surfaced + fixed
en route.

**Priority.** P2 — no product capability depends on it, but green CI is the gate
for trusting every phase's "verified" claim, and disk-exhausted lanes train
contributors to ignore CI. Continuation of [Phase 196](phase-196-ci-bring-up.md)
hardening, carved out because the disk/perf work is its own coherent unit.

**Depends on.** Phase 196 (the per-platform `platform-ci` matrix + dep-chain +
the CI-conventions baseline), Phase 218 (in-tree CLI build), RFC-0014
(`nros setup` provisioning).

---

## Levers (maintainer directives)

### 240.1 — Build/pull only what the run needs
- [x] **Push/PR build the `build-examples` smoke, not the full `build-all`.**
      `build-all`'s `build-fixtures` part (full lang×rmw matrix + cyclone cross
      builds) was the disk filler, yet those fixtures only feed the QEMU e2e —
      which is already on-demand. So push/PR build the lighter examples smoke
      ("core links clean"); `build-all` runs only on `schedule` /
      `workflow_dispatch` with `run_e2e`. Cuts disk + wall-clock on every push.
      (`platform-ci.yml` Build step; fallback `build-all || build-examples ||
      build` kept for platforms like qemu that have no `build-all` recipe.)
- Platform SDKs are already per-platform (`just <plat> setup` = `nros setup
  <board>` — only that board's toolchain/sources). The cross-cell `px4-rs` +
  `nuttx-libc` are small workspace-load deps (the shared workspace `Cargo.toml`
  patches libc to the NuttX fork + path-deps px4-sitl-tests), not platform SDKs.

### 240.2 — Shallow clone everywhere
- [x] **`--depth 1` on all CI submodule inits.** Every workflow's
      `git submodule update --init --recursive packages/cli/third-party/ros-launch-manifest`
      now fetches shallow (caps the recursive history). The main checkout
      (`actions/checkout@v4`) already defaults to depth 1, and `nros setup`
      sources default `shallow = true` (`--depth 1`, `sdk_store.rs`).

### 240.3 — Reclaim space once the binary is built
- [x] **In-container static-bloat reclaim before the heavy build.** Strip the
      image's `doc/man/locale` + apt lists (mirrors the host-integration-tests
      reclaim). `jlumbroso/free-disk-space` does NOT help container jobs — it runs
      inside the container and can't touch the host `/usr/share/dotnet` etc.
- **Finding — mid-build *source* reclaim has limited safe scope.** Fixtures share
      one cargo `--target-dir` (Phase 226.D — already lean, not per-example
      multiplied); the cmake C/C++ cell build dirs *hold the fixture binaries*
      (can't prune mid-build); and the cargo-cc SDK sources (zenoh-pico, mbedtls)
      are recompiled per cargo build (needed throughout). So the build-scope
      reduction (240.1) carries the disk win, not source-pruning. The big working
      trees (nuttx 614M, threadx 389M) are real content, provisioned only on the
      cells that need them.

### 240.4 — Preserve the on-demand full e2e (complete validation)
- [x] **The e2e-on-trigger feature stays alive.** `workflow_dispatch` with
      `run_e2e` (default true) + the nightly `schedule` (07:00 UTC) both drive the
      full `build-all` (fixtures) **and** the QEMU Test/e2e step — gated on the
      *same* `schedule || (dispatch && run_e2e)` condition, so the fixtures the
      e2e consumes are always built first. The 240.1 scope-split only diverted
      push/PR; the e2e path is untouched. Re-validated by dispatch runs (below).

## Regressions found + fixed via the e2e/CI runs

- [x] **lint** — `examples/native/rust/action-client/src/main.rs` had a >100-col
      `warn!`; wrapped (`cargo +nightly fmt`).
- [x] **colcon-parity** (never-green) — `colcon build` discovered the top-level
      nano-ros umbrella CMakeLists as a pure-CMake package (needs `nros`); added
      `--base-paths src` to scan only the parity-clean `src/`. First-ever pass.
- [x] **qemu `build-all` fallback** — the 240.1 e2e branch initially called
      `just <plat> build-all` with no fallback; qemu has no such recipe. Restored
      the `build-all || build-examples || build` chain.
- [x] **dep-chain** (196.6) green — `qemu-arm-baremetal` codegen tripped the
      `nros-core` abi_guard (CLI 0.5.0 vs the example's stale 0.1.0 `Cargo.lock` —
      known-issue #12); the resolution-only lane now sets `NROS_SKIP_VERSION_CHECK=1`.
- [x] **nros-node embedded compile** — Phase 239.1's service-client callback
      (`arena.rs`) called `Svc::Reply::deserialize` with `Deserialize` not in
      scope under `rmw-cffi`; fully-qualified it. Was the dominant platform-ci
      blocker (every embedded cell hit it before the disk-heavy build).

## Acceptance

- [x] Push/PR `platform-ci` cells do not run the full `build-fixtures` matrix.
- [x] All CI git submodule inits are shallow.
- [x] The on-demand e2e (`run_e2e` / nightly) still builds fixtures + runs the
      QEMU Test step (feature alive).
- [x] **Disk + compile levers validated on the heavy `build-all` path.** Dispatch
      `run_e2e` run 27393704883 (after the nros-node compile + qemu fallback fixes):
      esp32 fully green; **nuttx ran 9m46s — no `No space left on device`, passed
      the nros-node compile that killed every embedded cell at ~2–3 min in the
      prior run, and reached deep into the cpp `build-fixtures` matrix.** That is
      the disk/compile goal: the heavy cells no longer die on disk or the embedded
      compile. (Earlier run 27375218361 had qemu/nuttx/esp32 all fail at the
      *Build* step in 2–6 min — disk/compile; this run they get to the fixtures.)
- [ ] **Residual: nuttx cpp fixture header clash (NOT disk — separate owner).**
      nuttx's Test/e2e is red on a cpp compile clash, not disk: arm-none-eabi-g++
      building the cpp talker `nros-entry/main.cpp` hits
      `conflicting declaration 'typedef struct div_t div_t'` — newlib's
      `arm-none-eabi/include/stdlib.h` vs NuttX's own `stdlib.h` (both libc header
      sets on the include path for the C++ entry; issue-0027 made the NuttX sysroot
      win for the *C* message-lib path, but the cpp entry's cc-rs invocation still
      sees both). Same family as the cpp cases tracked in **issue #34** (honest-red
      e2e/integration now surfaces pre-existing cpp fixture bugs). Belongs to the
      nros-cpp / NuttX C++ header owner, not this phase. Phase-240's disk/build
      goal is met; the e2e-green box is gated on that fixture bug, filed against
      #34's cpp bucket.

## Notes / cross-refs

- Sibling disk work: issue-0029 (host-integration fixture-build disk exhaustion,
  resolved separately) + the host-integration-tests reclaim pattern this phase
  mirrors.
- The push/PR-vs-e2e split intentionally trades per-push fixture coverage for
  speed + disk headroom; the nightly `schedule` + on-demand `run_e2e` keep the
  full coverage available, surfacing drift without blocking pushes.
