---
id: 29
title: host-integration native fixture build exhausts runner disk (ENOSPC)
status: open  # fix applied, pending CI confirmation
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
CMake/Corrosion) + `build-workspace-fixtures`. Each carries its own target dir
full of debug symbols. Cumulatively this overruns the GitHub-hosted runner's
~14 GB OS disk.

**Constraint.** The job runs *inside a container* (`ghcr.io/newslabntu/nano-ros-ci`).
The usual "free disk space" actions delete host paths (`/usr/share/dotnet`,
`/usr/local/lib/android`, `/opt/ghc`) that are **not in the container's fs
namespace**, so they cannot help. The only space reachable from inside the job
is the mounted GitHub tool cache (`/opt/hostedtoolcache` + `/__t`, ~8 GB, unused
here — the rust/nightly toolchains are baked into the image at `/usr/local`)
and the build artifacts themselves.

**Fix applied (2026-06) — reclaim + shrink, keep full coverage.**
1. New `Free disk space (in-container reclaim)` step removes the mounted tool
   cache (`/opt/hostedtoolcache`, `/__t/*`) + `apt-get clean` before the build.
   Job runs as root in the ROS image, so no `sudo`.
2. The build step exports `RUSTFLAGS=-C debuginfo=0` — debug symbols are the
   dominant disk consumer in each fixture target dir, and the integration tests
   only *spawn* the binaries (no symbols needed). This shrinks the full
   rust + C/C++/Cyclone + workspace build enough to fit, keeping full fixture
   coverage rather than dropping the heavy cells.

`test-integration` itself only has `build-zenohd` as a prereq (it spawns the
prebuilt fixtures, does not rebuild them), so the test step neither refills the
disk nor re-compiles fixtures with mismatched flags.

Confirmation is the host-integration lane greening to test-integration on a run
that survives to completion. Archive once green.
