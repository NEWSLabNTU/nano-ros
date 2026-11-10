---
id: 2419
title: "No merge-gating lane compiled the uORB backend and its PX4 glue for a PX4 NuttX target, so issue 2403's fix reached main with three files PX4 could not compile"
status: resolved
type: tech-debt
severity: medium
area: [px4, nuttx, rmw, testing]
related: [phase-542, 2403, phase-547, 1437]
found: 2026-11-10
resolved_in: "the issue-2419 pull request: gate `px4-uorb-nuttx-compile` (fast tier) over a committed PX4 v1.17.0 snapshot"
---

**Home: [phase-542](../../roadmap/phase-542-px4-supported-platform.md)**, W10.
Found by [issue 2403](2403-px4-uorb-subscription-never-taken.md).

## What

Issue 2403's fix landed on main with `subscriber.cpp`, `session.cpp` and
`px4_callback_glue.cpp` in a state PX4 could not compile for NuttX: PX4 builds
NuttX C++ `-nostdinc++`, so `<atomic>` does not exist, and `-Wcast-align
-Werror` refuses a `reinterpret_cast` from the glue's byte storage. The next
commit on main fixed the files. Only the manual phase-547 Renode harness had
compiled them.

Nothing that gates a merge compiled `packages/rmw/uorb` for a PX4 target.
`check rmw-uorb` is a host build of the mock-ABI half and never compiles the
glue. The glue is compiled only inside a real `px4_add_module()`, which no CI
job runs.

## Fix

Gate `px4-uorb-nuttx-compile` (`scripts/check/check-px4-uorb-nuttx-compile.py`,
in the fast tier, so it runs in `check / fast` on every event). It compiles every
TU `nros_px4_add_module(BACKENDS uorb)` compiles, glue included, with
`arm-none-eabi-g++`, PX4's compile options for a NuttX module, and PX4's
headers.

- **Sources and nano-ros flags** are read, not restated: the TU list from
  `NrosRmwUorbSources.cmake` through `cmake -P` (issue 1437's one list), and the
  uORB arm's `COMPILE_FLAGS` / `INCLUDES` plus `_NROS_PX4_INCLUDES` from
  `integrations/px4/NanoRosPx4Module.cmake`.
- **PX4's flags and headers** come from a committed snapshot,
  `packages/testing/nros-px4-nuttx-compile/`, written by
  `scripts/gen-px4-nuttx-compile-snapshot.py` from a configured PX4 tree at the
  pin (`[source.px4-autopilot] ref`, v1.17.0, `px4_fmu-v6x_default`). CI has no
  such tree: the depth-1 `px4-autopilot` source has no NuttX submodule and no
  build dir, and the NuttX context (`config.h`, the arch links, uORB topic
  headers) exists only after a PX4 configure. The generator reads PX4's own
  `compile_commands.json` and takes the option set most stock
  `px4_add_module()` C++ TUs share (214 of 543), with the include roots they all
  share. It drops only `-fdiagnostics-color=always` and renames the per-module
  `MODULE_NAME` / `PX4_MAIN`. The headers are the ones `-M` reports the backend's
  TUs reaching, plus both NuttX `include/cxx` directories whole, since that file
  list is the C++ library a `-nostdinc++` build has. 173 headers (2.1 MB),
  verbatim with their licences, in one byte-reproducible `headers.tar.xz`
  (238 KB) that the gate unpacks into a temp dir. An archive, not a tree:
  committed as files, the headers were read as nano-ros code by four other
  fast gates (`weak-symbols`, `no-unbounded-condvar-wait`,
  `c-array-pool-floors`, `comment-names`) and would be reflowed by
  clang-format.
- **Staleness**: the gate fails if the snapshot names a PX4 ref other than the
  index pin, or if the archive's sha256 or member count differs from
  `snapshot.toml` (the archive is byte-reproducible, so it is hashed whole).
  Both halves are self-tested.
- **Negative controls** run first, on the same command line: `#include <atomic>`
  must fail as missing, a `reinterpret_cast` from `alignas` byte storage must
  fail `-Wcast-align`, and the `void *` spelling plus a plain `<cstdint>` +
  `<uORB/uORB.h>` TU must pass.
- **Compiler**: `$NROS_ARM_NONE_EABI_PREFIX-g++`, else `arm-none-eabi-g++` on
  PATH (the store's pinned 13.2 after `activate.sh`, else the CI image's apt
  package, which is also what PX4 v1.17's `Tools/setup/ubuntu.sh` installs).
  The gate prints which one. New lane capability `arm-gcc`, claimed by the
  `fast` and `build` lanes (the CI image).

## Measured

The negative control: the three files as issue 2403's first fix commit left
them, everything else at main, gate run unchanged:

| file | Arm GNU 13.2.rel1 (SDK store pin) | Ubuntu jammy apt 10.3.1 (the CI image's) |
|---|---|---|
| `session.cpp` | `fatal error: atomic: No such file or directory` | same |
| `subscriber.cpp` | `fatal error: atomic: No such file or directory` | same |
| `px4_callback_glue.cpp` | `cast from 'unsigned char*' to '{anonymous}::CallbackSlotImpl*' increases required alignment of target type [-Werror=cast-align]` | same |
| the other 5 TUs | compile | compile |

Gate exit 1 with both. The 10.3 run used the jammy `.deb`s unpacked into a
scratch dir, with their newlib headers added as `-idirafter` (the install puts
them at an absolute `/usr/include/newlib`).

On main, all 8 TUs compile and the 4 controls hold, with both compilers. The
gate takes about 0.3 to 0.4 s wall with 13.2 on a 4-CPU box (TUs in parallel).

## Limits

- One board (`px4_fmu-v6x_default`) and one compile-options set: a module's own
  `COMPILE_FLAGS` and its `STACK_MAX` (`-Wframe-larger-than`, 2048 here, the
  `px4_add_module` default) are not the nano-ros module's; nano-ros modules
  set a larger `STACK_MAIN`, so this limit is stricter than theirs.
- Compile only. It does not link, and it does not run anything.
- The snapshot holds only the headers main reaches. A new include of a header
  NuttX or PX4 does have, but the snapshot lacks, fails the gate until the
  snapshot is regenerated; the failure message says so. The C++ library list is
  complete, so a missing standard header is always the real thing.
- Not run by `just check fast` in a dev worktree (the `dev` lane does not claim
  `arm-gcc`); run it by name where a cross compiler is installed.
- Regenerating needs a PX4 tree and PX4's Python deps; it is not a gate, the
  same as `scripts/gen-upstream-edition.py`.
