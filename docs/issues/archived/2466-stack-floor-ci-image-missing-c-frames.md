---
id: 2466
title: "A fixture build's stack-floor walk read no frames behind its symlinked cargo dir, nor from libraries the image links after it is created"
status: resolved
type: bug
severity: medium
area: [memory, freertos, build, ci]
found: 2026-11-12
related: [issue-2400]
---

## What

Live-peer run 43153068357 (ci-image job 130967366665) ran the issue 2400 W2 stack-floor CHECK
on four FreeRTOS C++ cmake fixtures on mps2-an385:

- `minimal_publisher`
- `workspace_port`
- `topic_state_monitor`
- `rclcpp_compat_smoke`

The `--indirect fold` walk was refused for all four:

```
nros-stack-floor: REFUSED
root `service_server_is_available_trampoline`:
  - functions reached with no frame in any input (a missing frame would count as 0):
    zpico_graph_entry_at, zpico_graph_entry_count
```

The same image built locally, from
`examples/templates/cpp-port-minimal-publisher/mps2-an385-freertos` with `nros build`, gave a
different result. It has those frames: the fold completes, and the CHECK reports
"OPEN stack 65536 B holds the floor 15896 B (unmeasured)".

So one image, from the same sources, has a stack-floor input in one build environment and not in
the other.

## Why it matters

The walk refuses rather than counting a missing frame as 0, so no floor is computed low in
silence. But whether the CHECK can run at all then depends on the build environment rather than
the image:

- **An unmeasured image** degrades to a warning. That was the main red, fixed by #3586 / #3597.
- **A MEASURED image** in the same environment FAILS the build, by user ruling 2026-11-12
  ("go (c) and fail"). It would pass locally and fail in CI, or the reverse, for no reason in its
  code.
- A floor that sees a frame in one build and not the other is not one measurement.

## Where the frames come from

`zpico_graph_entry_at` / `zpico_graph_entry_count` are C, defined in
`packages/rmw/zenoh/zpico-sys/c/zpico/zpico.c`. cc-rs compiles that file inside the `zpico-sys`
build script, under the Corrosion import.

`nros_stack_floor_cargo_env` (`cmake/NanoRosStackFloor.cmake`) puts
`CFLAGS=-fstack-usage -fcallgraph-info=su` on that import, and the CHECK reads `.su` files with
`--su ${CMAKE_BINARY_DIR}`. A `.su` that is not written, or not under the binary dir, is a
missing frame.

## Hypotheses (NOT verified)

1. **sccache.**
   - The FreeRTOS road keeps `RUSTC_WRAPPER`. The comment says so: it "keeps … its sccache hits".
   - cc-rs uses an sccache `RUSTC_WRAPPER` as its C compiler wrapper too.
   - A cache hit restores the `.o`. Whether it restores the `.su` written beside it is unmeasured.
   - The ci-image has a warm sccache; a fresh local leaf may not.
   - The Zephyr carrier (W4) clears `RUSTC_WRAPPER` for its check.
2. **Target dir outside the binary dir.**
   - If the CI configure puts the cargo target dir (with cc-rs's `OUT_DIR`) outside
     `CMAKE_BINARY_DIR`, the `.su` files exist but are not read.
   - Then the missing frames would be every cc-rs function, not just two. The two the message
     names are only the first ones a fold path reaches.

## Next step

Re-run one of the four fixtures in the ci-image with `NROS_STACK_FLOOR` on, and check:

- whether `zpico.c`'s `.su` exists under the build's cargo `OUT_DIR`;
- whether that dir is under `CMAKE_BINARY_DIR`;
- whether `SCCACHE_*` / `RUSTC_WRAPPER` are set.

Then do the same with `RUSTC_WRAPPER=` empty. The answer decides the fix: clear the wrapper as W4
does, or name the cargo target dir to `--su`.

## Resolution (2026-11-12)

Measured. Both hypotheses above were wrong as stated. The CI image is not
special: the same failure reproduces on any host that builds the fixtures the
way CI does (`just freertos build-fixture-extras`). It does not reproduce under
`nros build`. Two independent defects, the second hidden behind the first.

### 1. The `.su` walk did not enter a symlinked directory

- The fixture recipe configures with `NROS_SHARED_CARGO_ROOT`. Each leaf's
  `<build>/cargo` is then a SYMLINK into `build/corrosion-cargo/<platform>/<key>`.
- The cargo target dirs therefore read as inside `build-zenoh`, as the CI log
  shows, while their content lives in the shared root.
- `load_su` collected frames with `Path.rglob`, which does not descend into a
  symlinked directory.
- So every `.su` a build script writes was never read, from zenoh-pico and the
  zpico shim to the platform C. Measured on `cpp-port-minimal-publisher`:
  `rglob` finds 67 `.su` files, none of them zpico's; following links finds
  423, of which 346 are zpico-sys's.
- The refusal named only two or three functions because the fold stops at the
  first address-taken function whose path reaches a frameless one. Which
  functions it named therefore varied per image.
- `nros build` makes `<build>/cargo` a real directory, which is why the same
  image walked locally.
- Fix: `frame_files()` walks with `os.walk(followlinks=True)` and visits each
  real directory once, so a link back up the tree cannot loop.
- Self-test: a frame behind a symlinked dir is read, and a loop is walked once.
  Restoring `rglob` fails the case.

### 2. Libraries linked after the image was created got no `-fstack-usage`

- `nros_stack_floor_check` ran when the image was created and took the image's
  link closure right then.
- A project links its own libraries AFTER `add_executable`.
  `ament-workspace-port` does `target_link_libraries(workspace_port
  counter_component)` two lines later.
- So `counter.cpp` was compiled without `-fstack-usage`, and once fix 1 let the
  walk past zpico it refused over
  `_ZN20workspace_port_nodes7CounterC1Ev`.
- Fix: the closure is walked by one `cmake_language(DEFER)` call at the end of
  the top-level configure. The images travel by GLOBAL property, because a
  deferred call's arguments expand empty.

### Measured after both fixes

The four leaves are built by the CI recipe, so through the symlinked cargo
dir. Every one walks:

| leaf | floor | stack |
| --- | ---: | ---: |
| minimal_publisher | 15,896 B | 65,536 B |
| rclcpp_compat_smoke | 20,968 B | 65,536 B |
| topic_state_monitor | 20,976 B | 65,536 B |
| workspace_port | 15,904 B | 65,536 B |

All four are UNMEASURED, so they warn. `minimal_publisher`'s 15,896 B equals
what `nros build` (no shared root) computes for the same image.

A missing frame was never counted as 0: the walk refused. So no floor was ever
computed low. The cost was that the CHECK could not run on these images, which
would have FAILED a measured one.

### The class: every reader of build output behind the symlink

Defect 1 is not specific to the stack floor. Any reader that walks a build dir
with `Path.rglob` or a bare `os.walk` misses everything behind `<build>/cargo`
whenever cargo dirs are shared.

Both readers now go through one helper, `scripts/lib/build_walk.py`
(`build_files()`). It follows symlinked directories and visits each real
directory once.

The sweep found one sibling: `scripts/build/dep-closure.py`, the MEASURED half
of a fixture's input signature.
- It read `*.d` dep-info, `build.ninja` and `Makefile.cmake` with `rglob`, so it
  never read cargo's dep-info.
- Measured on `cpp-port-minimal-publisher`'s fixture build dir: 109 repo paths
  before, 524 after. Every Rust crate source the image compiles was missing from
  the measured closure (`nros-node`, `nros-c`, `nros-cpp`, `nros-rmw-zenoh`,
  the generated message crates, ...).

Not siblings:
- **Tools that DELETE or count build output** must not follow a link out of
  their tree, and keep their own walk: `trim-caches`, `prune-superseded-artifacts`,
  `gc-shared-cargo` (which looks for the `cargo` links on purpose), and
  `check-example-leaf-target-dirs`.
- **`check-sizes-header-mirrors`** prunes `cargo`/`target` by design: no CMake
  target dir lives there.
- **`cargo-out-dir-headers`** walks inside one cargo `OUT_DIR`.
- **`nros-build-profile`** recurses with `Path::is_dir()`, which follows links.
- **The fixture staleness probe** walks SOURCE inputs, not build output.
- **`nros-mem-report`** reads one ELF; its glob is not recursive.

Sweep:
`git grep -n -E "\.rglob\(|glob\([^)]*\*\*|os\.walk\(" -- 'scripts/*.py' 'packages/*.py'`,
then each hit read for whether it walks build output.

