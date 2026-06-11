---
id: 29
title: host-integration native fixture build exhausts runner disk (ENOSPC)
status: open  # greened once (27345714715) then re-failed (27350995543); disk still marginal — reclaim step added
type: bug
area: build
related: [issue-0025, issue-0022]
---

When the `host integration-tests` lane is allowed to run the full native
fixture build to completion (it was previously masked — main-branch push churn
cancelled the long `Build native fixtures` step at ~2-4 min before it could
finish), the runner **runs out of disk**:

```
failure: Unhandled exception. System.IO.IOException: No space left on device
  : '/home/runner/actions-runner/cached/2.335.1/_diag/Worker_…utc.log'
```

The runner *agent itself* crashed writing its own diag log — i.e. the disk was
fully exhausted, not a test or compile failure. Confirmed on an isolated-branch
run (`ci/host-int-verify`, run 27326941199): every step through "Provision QEMU
+ XRCE agent + play_launch_parser" was green (so [issue 0025]'s `nros setup`
prereq work is proven), then `Build native fixtures` ran ~88 min and died on
ENOSPC.

**Cause.** `just native build-fixtures` = `build-fixture-rust`
(8 example crates × multiple RMW variants + TLS / bench / large-buf rows from
`examples/fixtures.toml`) + `build-fixture-extras` (C/C++ + CycloneDDS via
CMake/Corrosion) + `build-workspace-fixtures`, plus `test-integration`'s own
`cargo nextest` compile of the nros-tests crate + every integration test binary.

**Real root cause (corrected).** The first hypothesis — container disk
exhaustion — was *wrong*. The reclaim step's `df` showed the container overlay
`/` at **146 GB total, ~88 GB free** (40% used). The ENOSPC path in the crash
is `/home/runner/actions-runner/cached/<ver>/_diag/Worker_*.log` — the runner
**host's** small OS disk, a *different* filesystem from the container overlay.
GitHub streams every step's stdout/stderr to the agent log on that host volume,
and the ~100-minute verbose cargo+cmake build emits **gigabytes** of
`Compiling …` lines. The log itself overflows the host disk; the runner agent
then crashes writing its own diag log. So it is a *log-volume* problem, not a
build-artifact problem — and freeing container space (or stripping debuginfo)
does nothing for it.

**Second filesystem also exhausts — the container overlay.** The streamed log
showed `lto1: fatal error: write: No space left on device` + `compilation
terminated.` — GCC's LTO backend ran out of room writing transient objects. The
workspace `[profile.release]` is `lto = "fat"` (`Cargo.toml`) and the C++ FFI
fixture template (`cmake/cpp_ffi_Cargo.toml.in`) sets `lto = true`, so any
`--release` fixture + the C++ cross-language LTO writes huge `lto1` temp objects
on top of ~30 duplicated per-fixture target dirs — enough to fill even the
146 GB overlay. The old `|| echo "partial"` swallowed that ENOSPC, so the build
step went green with fixtures only partly built. Fix: force `CARGO_PROFILE_RELEASE_LTO=off`
for the CI fixture + test build (these are test fixtures; LTO is irrelevant to
pass/fail). The env var overrides both the workspace profile and the cpp_ffi
manifest, killing the transient and shrinking every target dir.

**Fix applied (2026-06) — keep the noisy output off the streamed agent log.**
Both heavy steps redirect their stdout/stderr to a file on the big overlay disk
(`build/ci-logs/*.log`) and surface only a tail (on failure for each fixture
sub-build; always, last 200 lines, for `test-integration` so its per-test
summary + real-failure verdict still show). The recipe exit code still gates the
step. `RUSTFLAGS=-C debuginfo=0` is kept only as a cheap artifact/link-time trim,
not as a disk fix. Full fixture coverage is preserved (the chosen
keep-full-build path).

(Superseded earlier attempts, for the record: an in-container tool-cache reclaim
step + `debuginfo=0` — both targeted the wrong filesystem and did not stop the
ENOSPC; they were replaced by the log-redirection fix above.)

**Still marginal after LTO-off — the C/C++ extras duplication (2026-06).** With
LTO off the lane greened ONCE (run 27345714715: `build-fixture-rust ok (49
lines)`, `build-workspace-fixtures ok (1337 lines)`, extras failed best-effort,
test-integration "treating as pass") — but the very next run on the identical
fix (27350995543) re-hit ENOSPC: `error: failed to build archive … libnros_cpp…
No space left on device (os error 28)` during `build-fixture-extras`, which then
left no room for test-integration's own compile → hard fail. The lane was
sitting right at the disk limit, so the outcome was a coin-flip.

Root of the residual pressure: `build-fixture-extras` rebuilds `nros-cpp` +
CycloneDDS into a **separate `build-cyclonedds/` tree per example**
(`examples/native/c/{talker,listener}`, `native/cpp/{talker,listener}`, …) — full
duplication of the dep graph, several GB each, that overruns the 146 GB overlay
even without LTO.

A reclaim-the-extras-trees attempt followed but only freed ~1.5 GB (the C/C++
`build-cyclonedds/` trees were small; the run had already filled the overlay
100% during `build-fixture-rust`). The reclaim `df` made the real culprit
plain — **the rust fixture build itself, not the extras**: `build-fixture-rust`
builds ~40 standalone per-example × RMW/feature variant rows, each its own
`target_dir` with a full dep-graph rebuild (zenoh-pico-sys, cyclonedds-sys, all
msg crates) → ~140 GB. That run also exposed a masking bug: 2 nros-tests
binaries `rustc-LLVM ERROR: No space left on device` (failed to *compile*), yet
the recipe printed "All failures were [SKIPPED] preconditions — treating as
pass" and exited 0 — a false green.

**Fix (2026-06) — scope the build (Option 3) + stop masking compile failures.**

1. **`--core-only` fixture scoping.** New manifest filter
   (`fixtures-manifest.py --core-only`, threaded through `fixtures-build.sh
   --core-only`) excludes rows that declare an isolated `target_dir` — i.e. the
   RMW/feature variant cells (TLS, safety-e2e, zero-copy, zenoh, xrce,
   large-buf). New recipe `just native build-fixture-rust-core` builds only the
   default-config per-example fixtures (native rust rows: 40 → 22). The
   host-integration lane uses it: its tests are the Phase 212.N/O workspace /
   launch / codegen shapes, which need the default fixtures + workspace
   fixtures; the variants are exercised by other lanes (platform-ci native
   cells, the RMW lanes) and `skip!` here via `NROS_FIXTURES_OPTIONAL`.

   **It is also a single ~146 GB disk** (the runner `_diag` log + the container
   overlay share it — earlier "two filesystems" framing was wrong), and a build
   that fills `/__w` kills the runner agent mid-step, *discarding that step's
   whole log*. So the three sub-builds were split into separate workflow steps,
   each with a `df` checkpoint that survives a later step's crash. That exposed
   the precise breakdown (run 27379181858):

   | stage | disk used | Δ |
   |---|---|---|
   | base (image + CLI + sources + provision) | 64 GB | — |
   | + rust-core (22 rows) | 81 GB | **+17 GB** ✓ |
   | + workspace fixtures | 93 GB | +12 GB ✓ |
   | + C/C++ extras | **145 GB (100%)** | **+52 GB** ✗ |

   So `--core-only` fixed the rust side (+17 GB), but **`build-fixture-extras`
   (C/C++ + CycloneDDS, rebuilt per C/C++ example) is the real ~52 GB hog** — it
   filled the disk to 100%, leaving test-integration 269 MB → ENOSPC. The lane
   therefore **does not run `build-fixture-extras`**: it is not this lane's
   purpose, was already failing to build on the issue-0027 nros-c posix clash,
   and its C/C++/Cyclone tests `skip!` here regardless. C/C++ coverage lives in
   its own lanes + platform-ci native cells. Peak now ~93 GB (64%), leaving
   ~53 GB for test-integration's compile.

2. **Compile-failure no longer masked.** `test-integration` /
   `_nextest-platform` now treat any `cargo nextest` exit ≠ 100 (or a missing
   junit) as a real build/setup failure and hard-fail, instead of running the
   [SKIPPED] tolerance. nextest exits 100 *only* when tests ran and some failed;
   101 (compile/build error, ENOSPC) is a setup failure that must not green a
   broken build. This is the guard that would have caught the false green above
   — and that surfaces any future disk regression as a real red.

Confirmation is the host-integration lane greening to test-integration on an
isolated `ci/host-int-verify` run (immune to the main-branch push churn that
cancels the long build step). Archive once green with the scoped build.
