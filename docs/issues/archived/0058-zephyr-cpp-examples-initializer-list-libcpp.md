---
id: 58
title: Zephyr C++ examples fail to build — `<initializer_list>` unavailable under minimal libcpp (regression from 242.3)
status: resolved
type: bug
area: c-api
related: [phase-242, phase-244]
---

`zephyr-dual-line.yml` cpp cells (`cpp/talker`, `cpp/listener`) fail on **both**
3.7 and 4.4 lines (zenoh overlay — CI builds `just zephyr build-one <ex> zenoh`).
Root cause:

```
/__w/nano-ros/.../packages/core/nros-cpp/include/nros/parameter.hpp:46:10:
  fatal error: initializer_list: No such file or directory
FAILED: CMakeFiles/app.dir/src/main.cpp.obj
```

## Root cause — recent regression

`packages/core/nros-cpp/include/nros/parameter.hpp:46` gained
`#include <initializer_list>` in commit **1808a8afb** (`feat(242.3):
fixed-capacity sequence parameters`, 2026-06-13) for `std::initializer_list`
ctors on the fixed-capacity sequence param type. Every cpp example pulls
`parameter.hpp` transitively via `<nros/nros.hpp>`.

The Zephyr cpp example `prj.conf`s set only `CONFIG_STD_CPP14=y`. That selects the
language standard but does **not** put the toolchain's libstdc++ headers
(`<initializer_list>`, `<vector>`, …) on the include path — Zephyr gates those
behind `CONFIG_REQUIRES_FULL_LIBCPP=y` (the minimal/subset C++ runtime ships
neither). Before 242.3, the cpp headers used only the subset available under
plain `CONFIG_STD_CPP14`, so the cells were green; the new include broke them.

Affected (matrix): `cpp/talker`, `cpp/listener` × {3.7, 4.4} = 4 jobs.
Not in matrix but same header dependency: `cpp/service-*`, `cpp/action-*`.

## Attempt 1 — `CONFIG_REQUIRES_FULL_LIBCPP=y` (REJECTED, reverted)

Added `CONFIG_REQUIRES_FULL_LIBCPP=y` to all `examples/zephyr/cpp/*/prj.conf`
(commit 63e19179b). CI (run 27477154407) showed it **does** resolve the
`<initializer_list>` error — but only to expose a worse one: full libcpp
reshuffles the libc/include resolution so the **host glibc** leaks into the C
compile of `packages/core/nros-platform-zephyr/src/platform.c`:

```
/usr/include/x86_64-linux-gnu/bits/types/timer_t.h:7:19:
  error: conflicting types for 'timer_t'; have '__timer_t' {aka 'void *'}
platform.c:244:45: error: 'CLOCK_MONOTONIC' undeclared
  (did you mean 'SYS_CLOCK_MONOTONIC'?)
```

`platform.c` relies on **transitive** POSIX decls (`CLOCK_MONOTONIC`,
`timer_t`) via `<zephyr/posix/pthread.h>`; under minimal libcpp that resolved to
Zephyr's POSIX, but full libcpp pulls host `/usr/include` and the two collide
(issue #42 class — host/std header clash). Net-negative: the original error hit
2 matrix cells; this one breaks `platform.c` for **every** cpp example. Reverted
in the follow-up commit.

## Fix — SHIPPED (commit f9a5c9041)

`<initializer_list>` is a *freestanding* C++ header — the compiler supplies the
`std::initializer_list` class shape; no library runtime is involved. The true
gap is that Zephyr's minimal libcpp (`lib/cpp/minimal/include`, used under
`-nostdinc++`) ships only `cstddef`/`cstdint`/`new`, and the example build adds
`-ffreestanding`.

Fix: add a freestanding `std::initializer_list` shim to the nano-ros-owned
`zephyr/cxx-compat/` include dir (already on the C++ include path via `-I`) —
same content/pattern as the existing
`packages/boards/nros-board-threadx-qemu-riscv64/cxx-compat/initializer_list`.
No host-libc bleed: the C platform TUs keep compiling against minimal
libcpp/picolibc.

**Verified locally on BOTH lines** (native_sim, host toolchain — provisioned via
`just zephyr setup --skip-sdk`, native_sim uses host gcc so the multi-GB SDK is
unnecessary; `ZEPHYR_TOOLCHAIN_VARIANT=host`): Zephyr 3.7 **and** 4.4 each build
`cpp/talker` + `cpp/listener` to `zephyr.elf` (EXIT=0); `platform.c` compiles
clean (minimal libcpp preserved, no host-glibc bleed). Marked resolved on that
basis — the build path used (`just zephyr build-one … zenoh`) is exactly the
dual-line lane's; the lane will re-confirm in-CI on its next run.

### Rejected (attempt 1, reverted)
`CONFIG_REQUIRES_FULL_LIBCPP=y` resolved the header but bled host glibc into
`platform.c` (`timer_t`/`CLOCK_MONOTONIC` clash — #42 class), breaking every cpp
example. Reverted in the follow-up commit; see the section above.

Found 2026-06-13; root-caused + fixed + verified (both lines) 2026-06-14.
Cross-ref #42. Note the dual-line lane's rust service/action cells stay red until
#59's image republish — that's a separate cause, not this issue.
