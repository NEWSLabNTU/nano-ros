---
id: 22
title: native-cyclonedds fixture build deadlocks (parallel corrosion→cargo on nros-c)
status: open
type: bug
area: build
related: [phase-226, issue-0012]
---

`just build-test-fixtures` (the `test-all` prerequisite) hangs for hours in the
`native-cyclonedds` leaf build and never completes, blocking `just test-all` on
the maintainer host.

**Symptom.** `build-fixtures-leaves` runs `just native build-fixtures`, which
invokes `scripts/build/fixture-make-driver.sh native-cyclonedds-cmake`. That
builds the C and C++ cyclonedds talker/listener fixtures *in parallel*
(`make -j`). Each fixture's CMake (corrosion) invokes:

```
cargo rustc --lib --target=x86_64-unknown-linux-gnu \
  --no-default-features --features=ros-humble,rmw-cffi,std,platform-posix \
  --package nros-c --crate-type=staticlib \
  --target-dir <example>/build-cyclonedds/cargo/nano-ros_1147c --release --locked
```

…which in turn spawns a nested `cargo build -p nros`. Two+ of these
`cargo rustc -p nros-c` processes (e.g. `examples/native/c/talker/...` and
`examples/native/cpp/listener/...`) sit in uninterruptible-sleep (`D`) state at
~0% CPU for hours with no progress; the build log stops advancing. An `sccache`
server (~33 threads in the process tree) is also live.

**Suspected cause.** Concurrent `cargo` invocations contending a shared lock —
the global `~/.cargo/.package-cache` lock and/or an `sccache` cache-dir / server
stall. The per-example `--target-dir`s differ, so it is not a target-dir
collision; the contention is on a *process-global* resource (cargo package
cache or sccache). Note all the `cargo rustc` calls share the **same crate**
(`nros-c`) and an identical `nano-ros_1147c` target-dir *suffix* across
different example roots, so they may also race on a shared corrosion/cargo
artifact.

**Observed (2026-06-10).** Two full runs:
- Run 1: deadlocked at `native-cyclonedds` after nuttx/qemu/threadx_linux/
  freertos reported OK. Killed after ~2 h.
- Run 2 (fresh `sccache --stop-server`): failed earlier on a freertos
  `rust-lld: duplicate symbol z_sleep_s / z_random_*` link error — most likely
  *stale artifacts* from the killed run 1 rather than a clean-build failure
  (freertos was OK on run 1). A clean rebuild would re-trigger this deadlock.

**Impact.** `just test-all` is unreachable on this host — fixtures can't be
stamped (`target/nextest/.fixtures-built`). Independent of, and on top of, the
stale-standalone-lock ABI-guard debt ([issue 0012](archived/0012-stale-standalone-lockfiles.md),
worked around with `NROS_SKIP_VERSION_CHECK=1`).

**Directions to investigate.**

1. **Serialize the native-cyclonedds leaves** — build C then C++ (or cap that
   group to `-j1`) so two `cargo -p nros-c` builds never run concurrently.
   Cheapest mitigation if the contention is cargo-global.
2. **Disable sccache for the fixture build** (`RUSTC_WRAPPER=` / `SCCACHE=0`)
   to rule out an sccache server stall.
3. **Confirm the lock** — when hung, inspect the `D`-state cargo with
   `cat /proc/<pid>/wchan` + `ls -la ~/.cargo/.package-cache` + `lslocks` to see
   whether it is the cargo package-cache flock or sccache.
4. **Per-leaf isolated `CARGO_HOME`/target** so concurrent leaves don't share
   the package-cache lock at all.

**Reproduce.**
```bash
source ./activate.sh
NROS_SKIP_VERSION_CHECK=1 just build-test-fixtures   # hangs at native-cyclonedds
# or scope it:
scripts/build/fixture-make-driver.sh native-cyclonedds-cmake
```
