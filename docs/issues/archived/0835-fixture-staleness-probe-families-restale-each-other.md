---
id: 835
title: "`check-fixtures-stale` reported the same rows on every run — NOT the two
  families re-staling each other, but cells that could not build (riscv64-threadx
  had the wrong NetX config dir) plus a probe keying on cargo's `\"fresh\":false`"
status: resolved
type: bug
area: testing
related: [issue-0828, issue-0196, issue-0466, issue-0460, issue-0491, issue-0820, phase-344, phase-340]
resolved_in: nros_board_toolchain_env (cmake/NanoRosCargoProfile.cmake)
---

## Problem, as originally reported

*The hypothesis in this section is wrong — kept because it is what the counts
looked like, and why. See "Verdict" below.*

`scripts/check-fixtures-stale.sh` probes two families in order: cmake cells
first, then rust fixtures. Each family "self-heals" — it rebuilds what it finds
stale. **Rebuilding either family makes the other stale**, so a full run always
reports work and the tree is never in a state where both are fresh.

Measured on a tree where `just build-test-fixtures lane=all` had just completed
green (all nine legs OK):

```
run 1: 17 C/C++ cell(s) STALE and rebuilt · 23 rust fixture(s) STALE and rebuilt
run 2: 17 C/C++ cell(s) STALE and rebuilt · 23 rust fixture(s) STALE and rebuilt
run 3: 17 C/C++ cell(s) STALE and rebuilt · 23 rust fixture(s) STALE and rebuilt
```

Identical counts, identical membership. Not a treadmill converging — a fixed
oscillation.

The per-row probe DOES converge, which is what makes this hard to see:

```sh
bash scripts/test/rust-fixture-stale.sh "$row"   # prints the row  → stale
bash scripts/test/rust-fixture-stale.sh "$row"   # prints nothing  → fresh
```

And immediately after a full run — in which a cmake cell was rebuilt, and then
the rust family was rebuilt after it — that same cmake cell is stale again:

```sh
bash scripts/test/cmake-fixture-stale.sh "$c_row"
# examples/qemu-riscv64-threadx/c/talker/build-zenoh
```

So the rust rebuild writes something the cmake cells' input signature covers.
The two families share cargo outputs (the rmw staticlibs, the phase-340 shared
cargo group directory), and the signature on one side counts an artifact the
other side legitimately rewrites.

## Consequence

This is what keeps `just ci-matrix` red. The lane gate self-heals in-lane rows,
which stales out-of-lane rows that the lane never builds and — per issue 0828 —
cannot skip, and `test-all` then fails ~190 tests with

```
Workspace fixture <id> is stale: …/.nros-workspace-fixture.<id>.inputsig
```

Every one of those tests passes when run solo afterwards, because by then the
family it needed has been healed. Two consecutive `ci-matrix` runs on either
side of an unrelated change produced **the identical 92-test failure set** —
which is the signature of a build-state problem rather than a code one, and is
how it gets misattributed to whatever landed most recently.

## Fixed on the way: the probe's row selection

The rust probe selected rows by `--lang rust` while `is_cargo_row` has been
BUILDER-keyed since phase-344 W2. The twelve `examples/qemu-riscv64-threadx/rust/*`
rows (six zenoh, six cyclonedds) are `lang = "rust"` with `builder = "cmake"`,
so they were handed to `cargo build`, which cannot build a threadx cmake leaf:

```
ERROR: 12 rust fixture(s) could NOT be built by the staleness probe
  error: could not compile `nros-rmw-zenoh-staticlib` (lib)
```

A row the probe cannot build is never fresh, so those twelve were stale on every
run forever. They were also in the cmake list, where they self-healed correctly,
so each was reported twice under two labels — and the ERROR block named them
`build-zenoh` with no leaf path, which read as unattributable rather than as a
partition bug. Now `--builder cargo`, matching `is_cargo_row`. That removed the
ERROR block; the oscillation above is what remains.

## Directions

1. **Find the shared artifact.** The candidates are the rmw staticlibs and the
   phase-340 shared cargo `--target-dir` group: an output one family produces
   and the other's `.inputsig` hashes. Whichever it is, one side is treating a
   BUILD OUTPUT as an INPUT.
2. **Probe order must not matter.** A gate whose result depends on which family
   it looked at first is not measuring freshness. Once (1) is known, either
   exclude the shared output from the signature or make both families derive it
   from the same producer.
3. **Until then, `check-fixtures-stale` cannot be trusted to mean "the tree is
   stale"** — it means "someone rebuilt something". Its self-heal makes each run
   look successful, which is why this survived: the WARNING path reads as the
   gate working.

## Sweep

```sh
grep -rn 'inputsig' scripts/test/*.sh scripts/build/*.sh | head
grep -rn 'nros_fixture_target_dir_flag\|cargo-fixtures' scripts/
```

## Root cause of the cmake half — found 2026-08-29, and it is not oscillation

The eight `examples/qemu-riscv64-threadx/**` cmake cells were reported stale on
every run because **they do not build at all**. `cmake --build` returns 101,
three times running, artifact unchanged:

```
third-party/threadx/netxduo/common/inc/nx_api.h:155:10:
    fatal error: nx_port.h: No such file or directory
```

The probe is right to report a failing cell as stale — it was hardened for
exactly that ("A cell that does not BUILD is not fresh"). So the identical
count on every run is not a fixed oscillation between two families; it is the
same cells failing the same way, and the rust family is not implicated in it.
Checking determinism first would have shown this in one command: three
consecutive builds, byte-identical artifact, non-zero exit every time.

### Why it failed

NetX Duo ships no riscv64 port, so `nx_port.h` comes from the BOARD:
`packages/boards/nros-board-threadx-qemu-riscv64/config/nx_port.h`.

The failing translation unit is on the CARGO side — `zpico-sys`'s build script
compiling `c/zpico/zpico.c` through cc-rs, not a cmake-compiled file. Its
include list carried

```
-I .../packages/boards/nros-board-threadx-linux/config      <- THREADX_CONFIG_DIR
-I .../packages/boards/nros-board-threadx-linux/config      <- NETX_CONFIG_DIR
```

twice, and the riscv64 config dir nowhere.

**The value was not missing. It was wrong, and it came from the ambient
environment.** `just/sdk-env.just` exports `THREADX_DIR`, `THREADX_CONFIG_DIR`,
`NETX_DIR` and `NETX_CONFIG_DIR` unconditionally for every recipe, each
defaulting to the **threadx-linux** board:

```just
export THREADX_CONFIG_DIR := env("THREADX_CONFIG_DIR", justfile_directory() / "packages/boards/nros-board-threadx-linux/config")
export NETX_CONFIG_DIR    := env("NETX_CONFIG_DIR",    justfile_directory() / "packages/boards/nros-board-threadx-linux/config")
```

direnv/`activate.sh` puts them in an interactive shell too. So
`nros-zpico-build`'s `if let Ok(dir) = env::var("NETX_CONFIG_DIR")` SUCCEEDED —
with threadx-linux's path — and the two vars sharing one default is why the
same `-I` appears twice.

`cmake/board/nano-ros-board-riscv64-qemu.cmake` names the right directory and
publishes it with `set(ENV{NETX_CONFIG_DIR} …)`, which mutates only the
CONFIGURE-time process. The build runs later, under the user's own shell, where
the global default is what cargo inherits. A per-target override was never
attempted, so the board's value could not win.

That distinction matters for anyone reading this later: the failure mode is a
GLOBAL DEFAULT SHADOWING A PER-BOARD VALUE, not an unset variable. An
"is it set?" guard in the build script would have reported everything fine.

**This is issue 0460 one lane over** — there a Kconfig knob reached the Zephyr C
lane and not the Rust one, same `set(ENV{})`, and the Rust lane took a default
instead. The twist here is that the default is not the build script's own: it is
exported repo-wide by `just/sdk-env.just`, so it looks like configuration rather
than a fallback, and it is present on every board including the ones it is wrong
for. The file even carries a comment saying it patches the process env "so
cargo invocations spawned by corrosion … see the RISC-V board's config dir",
which is the thing that does not work.

Proof, same tree, same cell — the difference is the VALUE, not presence:

| `NETX_CONFIG_DIR` / `THREADX_CONFIG_DIR` | `cmake --build` |
| --- | --- |
| ambient default (`nros-board-threadx-linux/config`) | **101** |
| overridden to `nros-board-threadx-qemu-riscv64/config` | **0** |

### Fix

`nros_board_toolchain_env(<target>)` in `cmake/NanoRosCargoProfile.cmake` —
sibling of `nros_cargo_profile_env`, forwarding `THREADX_CONFIG_DIR`,
`NETX_CONFIG_DIR`, `THREADX_PORT`, `THREADX_EXTRA_INCLUDES`, `THREADX_DIR`,
`NETX_DIR` and `THREADX_BOARD_DIR` through `corrosion_set_env_vars` (normalised
by `nros_corrosion_env_target`, issue 0657, so it lands on the INTERFACE target
the cargo command actually reads). It checks ENV as well as the cache because
two of those are published ONLY as process env.

Wired at the sites that import the crates cargo builds for a cell:
`packages/api/nros-{c,cpp}/CMakeLists.txt` (the C/C++ example cells),
`cmake/NanoRosRuntimeCrate.cmake` and the riscv64 board's own rust-app path.

Verified with the variables UNSET in the environment: `cmake --build` now
returns 0 and `build.ninja` carries `NETX_CONFIG_DIR` once per corrosion
target. A native cell still builds green.

### The cmake half converges — measured over all 120 configured cells

Every configured C and C++ cell, built incrementally, verdict by exit status
and by md5 of the leaf's executables (`tmp/cells835.sh`):

| run | ok | CHANGED | FAIL |
| --- | --- | --- | --- |
| 2 (c half's first post-fix build) | 94 | 25 | 1 |
| 3 (settled) | **119** | **0** | 1 |

Zero CHANGED on a settled tree. The cmake family reaches a fixed point, so the
"17 C/C++ cells stale on every run" in the report above was not oscillation
against the rust family — it was cells that could not build, plus the ordinary
first rebuild after a change.

Two method notes, because both bit while measuring this:

* `fixtures-manifest.py --lang` is SINGLE-VALUED. `--lang c --lang cpp` silently
  keeps only `cpp`, so a sweep written that way covers half the cells and reads
  as complete. (`check-fixtures-stale.sh` itself is fine — it makes two separate
  calls.) The first sweep run here had this bug and probed no C cell at all.
* A sweep is only meaningful on a SETTLED tree, and "settled" is per-half: run 2
  was clean for cpp (already rebuilt) and confounded for c (first build after
  the cmake fix). Run 3 is the one to quote.

### The one cell still failing is a DIFFERENT defect

`examples/qemu-riscv-nuttx/c/talker/build-zenoh` fails to link, both runs:

```
nuttx-apps/netutils/ping/icmp_ping.c:496:(.text.icmp_ping+0x322):
    undefined reference to `__wrap_poll'
error: could not compile `nros-nuttx-riscv-ffi` (bin "nros-nuttx-ffi")
```

That leaf carries `-C link-arg=-Wl,--wrap=poll` (archived issue 0167 — bridging
Rust's 8-byte `pollfd` to NuttX's 24-byte kernel struct), and the interposer it
names is `__wrap_poll` in the patched libc fork
(`third-party/nuttx/libc`, `src/unix/nuttx/mod.rs:645`), which does carry
`#[no_mangle]`. So the symbol exists and is not reaching the link — not
investigated further here.

It is the SAME CLASS as the riscv64-threadx cells above (a cell that cannot
build is reported stale forever) but a different cause, and it is the leaf
issue 0820 is already open on, where the reported symptom was a museum binary
rather than a link failure. Whoever takes 0820 should start here.

### Class sweep — no sibling sites

Per the "fix the class" rule, both halves of the pattern were enumerated rather
than assumed:

```sh
git ls-files 'packages/boards/*/config/*' | grep -E 'nx_port\.h|tx_port\.h|nx_user\.h|tx_user\.h'
grep -rn 'set(ENV{THREADX_CONFIG_DIR\|set(ENV{NETX_CONFIG_DIR' cmake/
```

Exactly two boards carry a ThreadX/NetX config dir — `nros-board-threadx-linux`
(the one the global default names, so it is correct for itself) and
`nros-board-threadx-qemu-riscv64` (this bug) — and exactly one cmake file
publishes the variables. So there is no sibling site to fix alongside.

The hazard that remains is structural, not a second instance: because
`just/sdk-env.just` exports a default that is right for one board, **the third
ThreadX board will inherit threadx-linux's config silently**, exactly as this
one did. Making the default absent (and the build script fail loudly on an unset
`NETX_CONFIG_DIR`) would turn that from a wrong-header compile into a named
error. Not done here — it is a change to a repo-wide env contract and belongs
with whoever adds that board.

### Verdict: both families converge — the mutual re-staling is refuted

`scripts/check-fixtures-stale.sh` run twice back to back, whole scope, settled
tree:

| family | run 1 | run 2 |
| --- | --- | --- |
| C/C++ cells (cmake, self-healing) | 1 stale | **1 stale** |
| rust fixtures (cargo, self-healing) | 41 stale | **0 — no warning at all** |
| workspace fixtures (ERROR, no self-heal) | 17 | 17 |
| compile-check fixtures (ERROR, no self-heal) | 22 | 22 |

The rust family drops to zero on the second pass. It does not re-stale after the
cmake family was rebuilt ahead of it, which is the exact behaviour this issue was
opened on. The 41 in run 1 are a ONE-TIME invalidation from the env-var fix above
(new entries in every corrosion target's cargo fingerprint) — predicted, and it
cost one rebuild and no more, as it should.

The residual `1` on the cmake side is the un-linkable riscv-nuttx cell, not a
cell that keeps going stale.

The workspace and compile-check families are not part of this: they do not
self-heal (they ERROR and tell you to build), and on this tree 13 of the 17
workspace rows say **missing** because `build-workspace-fixtures` was never run
here. Identical counts across two runs are expected for a family nothing is
rebuilding, and prove nothing either way.

So the title's claim is wrong. What actually produced the repeating 17/23 was two
separate things, both now addressed:

1. cells that **could not build** and were therefore stale forever — the
   riscv64-threadx family, fixed above;
2. the rust probe keying on cargo's `"fresh":false`, which counts a re-run UNIT
   as a stale ARTIFACT — already hardened to compare artifact bytes (see the
   comment block in `scripts/test/rust-fixture-stale.sh`).

Neither is two families fighting each other. Both look like it from the counts
alone, which is why the counts alone were not enough.

### What this does NOT settle

* The end-to-end symptom — `just ci-matrix` failing ~190 tests — has NOT been
  re-run. The mechanism behind it is measured and fixed; the lane itself takes
  hours and was not exercised. Anyone closing the loop should run it before
  quoting this issue as the reason the lane went green.
* `examples/qemu-riscv-nuttx/c/talker/build-zenoh` still cannot link
  (`__wrap_poll`). Tracked with issue 0820, whose leaf it is.
* One thing to watch: this sets path-valued env vars on any corrosion target
  for which at least one is defined — which, because `just/sdk-env.just` exports
  all four unconditionally, means every target, not only the ThreadX ones. A
  freertos cell now carries `NETX_CONFIG_DIR=…/nros-board-threadx-linux/config`
  in its cargo fingerprint; it already inherited the same value ambiently, so
  this is not a behaviour change, but it is newly RECORDED per target. That is
  the shape issue 0491 warns about (a PATH variable in a cargo fingerprint). The
  values are constant per leaf, so it should cost one rebuild and no more — and
  the 41→0 measurement above is consistent with exactly that, though attributing
  those 41 specifically to this change is inference, not something measured. If
  fixtures start re-staling in a way that tracks these variables, look here
  first.
