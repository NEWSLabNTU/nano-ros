---
id: 3019
title: "A configured build keeps the source tree it located first: moving a submodule checkout off its pin rebuilds the old tree"
status: resolved
type: bug
severity: high
area: [build, cmake, provisioning]
related: [issue-3010]
found: 2026-11-28
resolved: 2026-11-28
resolved_in: "fix(build): a configured build re-configures when a located source tree moves"
---

## What happens

A cmake-road build directory keeps building the source tree the locate
ladder chose when it was first configured, after the ladder's answer has
changed. The image is rebuilt from the OLD sources, reports success, and
nothing says which tree it compiled.

Observed by the ASI campaign on `freertos-posix` (cmake road,
`nros build controller_bringup:freertos-posix`), 2026-11-28:

1. `nros setup --source freertos-kernel` left the store copy
   `freertos-kernel/10.6.2+0adc196d` in place, and the first build located
   the kernel there (store rung).
2. `git checkout c36570126` inside the project's
   `third-party/freertos/kernel` submodule. `nros locate freertos-kernel
   --why` then chose the local-edit rung ("is at c3657012 (pin 0adc196d)
   <- chosen"), and `nros-sdk.lock` was rewritten to local-edit c3657012.
3. `nros build` again, same build directory: the image was
   **byte-identical** to the one from step 1. Only deleting the build
   directory produced an image with the new kernel (its `nm` shows the
   patch's `ullTicksDue`).

So every tool that reports a location named the checkout, and the build
did not compile it.

**Mirror run, same directory, no wipe** (Makefile generator, so the
compile lines were read from `compile_commands.json` and `build.make`).
The directory held the step-3 build (submodule path); the kernel
checkout went back to the pin, and `nros locate` moved to the store rung
(`~/.nros/sources/freertos-kernel/10.6.2+0adc196d <- chosen`). The next
`nros build` printed `configure: up to date`; `CMakeCache.txt`, `Makefile`
and `Makefile.cmake` kept their mtimes; `FREERTOS_STARTUP_INCLUDES` and the
`tasks.c` compile line still named the submodule. The image was relinked
from the submodule's (now pin) files: it matched neither reference image.
In that direction the result only looks plausible because the files
changed under the path cmake kept. In the original direction the kept
path was the store copy, which nothing changed, so nothing rebuilt.

## Why

`cmake/NanoRosLocate.cmake` asks the ladder once per CONFIGURE
(`nros locate --all --format cmake`) and registers exactly one file as a
configure input: `nros-sdk-index.toml`. The ladder
(`nros_build_paths::locate::resolve`) reads more than that:

| rung | what it reads | a configure input? |
|---|---|---|
| env | `$FREERTOS_DIR` (etc.) | no |
| project | the project's `system.toml` `[sources]` | no |
| local-edit | whether the submodule checkout is off the pin | no |
| store | whether `<store>/<name>/<version>+<ref>` exists | no |
| checkout | whether the submodule is initialised | no |
| (pins) | `nros-sdk-index.toml` | **yes** |

A change to any input other than the index changes the ladder's answer
without re-running configure, so the generated build files keep the old
tree's paths. Two layers decide "configure is up to date", and neither
sees those inputs: `nros build`'s own check (it printed `configure: up to
date`) and cmake's `CMAKE_CONFIGURE_DEPENDS`. The build tool then rebuilds correctly against those paths: the edge it
lacks is upstream of it, at configure. Same class as issue 1018 (a
configure-time emitter with no `DEPENDS` for its tool).

## Consequence

High, because the ordinary ways a contributor or a CI cache changes a
source tree all take this path: checking out a fix in a submodule,
`nros setup --source` provisioning a store copy, adding a `[sources]`
row, setting or unsetting `FREERTOS_DIR`. The build passes and runs the
old code. A pin bump that moves `nros-sdk-index.toml` does reconfigure,
so it re-resolves; the local-edit and store transitions do not.

## Acceptance

- After any one of those input changes, the next build of an existing
  directory compiles the newly located tree. Checked without a wipe:
  `ninja -t query` / the compile lines name the new path, and the image
  differs.
- The fix covers every caller of `nros_locate_var` / `nros_locate_source`,
  not only FreeRTOS: the ladder serves every vendored source row.

## Resolution

Both layers now see what the ladder read.

- `nros locate --all --format cmake` states the answers' INPUTS
  (`NROS_LOCATE_INPUTS`, from `locate::watch_inputs`): the project's
  `system.toml`, each store row's directory (or its nearest existing
  ancestor, so a copy appearing is seen), and each submodule checkout with
  its git `HEAD`, the ref `HEAD` names and `packed-refs` (from `git
  rev-parse --git-path`). Only paths that exist are stated, so a missing one
  never makes every build re-configure.
- `cmake/NanoRosLocate.cmake` registers them in ONE helper,
  `_nros_locate_register_inputs`, called from the loader every
  `nros_locate_var` / `nros_locate_source` goes through, so no caller adds
  or forgets its own. It also writes `nros-locate.stamp` in the build dir:
  the CLI, index and project asked, and the answers.
- `nros build`'s configure stamp re-asks the ladder with that index,
  project and the handoff's environment, and compares the answers before it
  skips a configure. That covers what no file states: a row's environment
  variable (four of them, `LWIP_DIR`, `NETX_DIR`, `PX4_AUTOPILOT_DIR`,
  `TONBANDGERAET_DIR`, matched none of the stamp's env prefixes) and a
  checkout's uncommitted edits.

A plain `cmake --build` outside `nros build` still cannot see those last
two; re-run `cmake <build-dir>` after changing a source override variable
or dirtying a pinned submodule by hand.

Gate: `check-locate-configure-inputs` (fast line). No cmake file asks the
source ladder outside `NanoRosLocate.cmake`, the loader calls the helper,
and a self-test configures a real cmake + ninja project with a stand-in CLI:
a no-change build does not re-configure, and moving a stated submodule
`HEAD` or store directory does. Both mutations of the helper (not calling
it; not registering the inputs) turn it red.

Measured with the real CLI and kernel, one build dir, no wipe:

| step | `cmake --build` | located `FREERTOS_DIR` |
|---|---|---|
| configure, kernel at the pin | — | store `10.6.2+0adc196d` |
| no change | no re-configure | — |
| `git checkout` the fork's commit in the submodule | `Re-running CMake` | the checkout |
| back to the pin | `Re-running CMake` | store `10.6.2+0adc196d` |

