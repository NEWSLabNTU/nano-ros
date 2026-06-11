---
id: 29
title: host-integration native fixture build exhausts runner disk (ENOSPC)
status: resolved  # host-integration lane green end-to-end (run 27345714715)
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

Confirmation is the host-integration lane greening to test-integration on a run
that survives to completion (verified on the isolated `ci/host-int-verify`
branch, immune to the main-branch push churn that cancels the long build step).
Archive once green.
