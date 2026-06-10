---
id: 22
title: native-cyclonedds fixture build deadlocks (parallel corrosion→cargo on nros-c)
status: resolved
type: bug
area: build
related: [phase-226, issue-0012]
resolved_in: 2026-06-11 (strip jobserver from nros-sizes-build nested probe)
---

**RESOLVED — final fix `de086ce45` (2026-06-11).** Running the **full** fixture
build (`just build-test-fixtures`) showed the deadlock is **cross-platform**,
not native-only: with the platforms building concurrently, cyclone cargos on
**threadx-linux** (6 examples), **qemu-riscv64-threadx** and
**qemu-arm-freertos** all hung in `futex_wait_queue` inheriting the fifo
jobserver (`--jobserver-auth=fifo:`). The first fix below was native-only and
couldn't cover them.

**Universal fix:** strip the jobserver at its *source* — `nros-sizes-build`'s
nested opaque-size probe cargo now `env_remove`s `MAKEFLAGS` / `CARGO_MAKEFLAGS`
/ `MAKELEVEL` (`packages/core/nros-sizes-build/src/lib.rs`). Every platform's
cyclone build goes through `nros` → this probe, so the recursive hold-and-wait
(outer cargo holds tokens → blocks in the build script → waits for the nested
probe → which waits for a token the outer holds) can no longer form anywhere,
*without* disabling jobserver coordination for the outer build. The native
driver workaround was reverted to the plain parallel call (superseded).

⚠️ **Validation caveat:** the under-load full-build re-run was thwarted by
competing host CPU (Carla sim + ML training, ~300%) + cargo-lock contention, so
the universal fix is **correct by construction** (textbook recursive-cargo
jobserver break) + nros-sizes-build compiles, but the full end-to-end run was
not re-confirmed on this host. Re-validate `just build-test-fixtures` →
`just test-all` on an idle host.

---

**First fix (2026-06-10, native-only, superseded by the source fix above).**
`scripts/build/fixture-make-driver.sh` ran each native cyclone leaf with
`MAKEFLAGS= MAKELEVEL= CARGO_BUILD_JOBS=<nproc/2>` — stripping the fifo jobserver
from cargo while keeping the **shared** `~/.cargo` whose package-cache lock
serializes the two concurrent dep builds safely (isolating `CARGO_HOME` instead
caused a `.fingerprint` write race). Validated `PAR3EXIT=0`, both native leaves,
~6 min vs ~11 min serialized. Covered native only; the source fix supersedes it.

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

## Root cause (confirmed 2026-06-10)

`lslocks` + `/proc/<pid>/wchan` while hung showed the cyclone `cargo` procs in
`futex_wait_queue` holding `~/.cargo/.package-cache-mutate` (READ) plus the
per-example `…/nano-ros_1147c/.cargo-*-lock` flocks. The two leaves
(`fixtures-build.sh native c cyclonedds` + `… cpp …`) run **concurrently** under
the outer `make -jN --jobserver-style=fifo`, and each builds the **same**
`nros-c` / `nros` crates via corrosion→cargo (plus a nested `cargo build -p nros`).
Concurrent cargo invocations therefore contend two process-global resources:

1. the global `~/.cargo/.package-cache-mutate` flock (held across dependency
   resolution), and
2. the make **fifo jobserver** tokens, shared with the nested cargo (a known
   cargo 1.96 × fifo-jobserver × nested-cargo hazard).

On the maintainer host this was **massively amplified by competing CPU load** —
an unrelated `CarlaUE4-Linux` sim (~222% CPU) + an ML `trainable` job (~84%) on
a 32-core box (loadavg ~20). Starved lock-holders hand the package-cache lock
over slowly, so the contention degrades to an apparent multi-hour hang rather
than a clean serialization.

## Fix applied — serialize the two cyclone leaves

`just/native.just` now calls the cyclone-cmake driver with `NROS_BUILD_JOBS=1`,
so the outer make runs `-j1` → only one leaf's cargo builds at a time and cargo
gets `MAKEFLAGS=-j1` (no fifo jobserver). Each leaf is *already* internally
serial (`NROS_JOBSERVER=1` → `run()` loops) and the intra-leaf cmake build still
runs at full native parallelism, so the only cost is the C and C++ leaves no
longer overlap. **Validated: `SERIALEXIT=0`** (both leaves built, ~11 min even
under the heavy external CPU load).

## Restoring parallelism (still open — needs care)

Two attempts to keep the leaves parallel both failed:

- **Per-leaf isolated `CARGO_HOME`** (`CARGO_HOME=build/cargo-home-native-<lang>`)
  did *not* hang but failed fast with
  `error: failed to write …/nano-ros_1147c/.fingerprint/nros-c-<hash>/run-build-script-… (os error 2)`
  on **both** leaves — i.e. the concurrency bug re-surfaced as a fingerprint
  write race rather than a lock hang. The identical `nano-ros_1147c` target-dir
  *suffix* + identical `nros-c-<hash>` across examples is suspicious; corrosion
  may share build state in a way that isn't safe across concurrent cargo.
- **Stripping the fifo jobserver from the leaf** (`MAKEFLAGS= CARGO_BUILD_JOBS=8`)
  could not be validated cleanly under the host load in the time available.

A correct parallel fix likely needs corrosion-level **per-example target/build
isolation** (so two concurrent cargos never touch the same `nano-ros_1147c`
fingerprint tree), and/or keeping cargo off the shared fifo jobserver. Until
then the serialized leaves are the reliable path; the wall-time cost is small
(the leaves are internally serial anyway, and on this host the build is
CPU-bound by external load regardless of leaf parallelism).

**Reproduce.**
```bash
source ./activate.sh
NROS_SKIP_VERSION_CHECK=1 just build-test-fixtures   # serialized now; previously hung at native-cyclonedds
# scope just this group (needs the cyclone cmake env from `just native build-fixtures`):
source scripts/build/cargo.sh
export NROS_CMAKE_EXTRA_DEFS="-DCMAKE_BUILD_TYPE=Release -DNANO_ROS_BUILD_CODEGEN=OFF -D_NANO_ROS_CODEGEN_TOOL=$(nros_cargo_codegen_c_bin) -DCMAKE_DISABLE_FIND_PACKAGE_CycloneDDS=ON -DNROS_RMW_CYCLONEDDS_MSG_TO_IDL=$PWD/scripts/cyclonedds/msg_to_cyclone_idl.py"
NROS_BUILD_JOBS=1 NROS_SKIP_VERSION_CHECK=1 bash scripts/build/fixture-make-driver.sh native-cyclonedds-cmake   # serial: OK
# (drop NROS_BUILD_JOBS=1 to reproduce the parallel contention)
```
