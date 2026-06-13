---
id: 58
title: Zephyr C++ examples fail to build — `<initializer_list>` unavailable under minimal libcpp (regression from 242.3)
status: open
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

## Fix direction (open — needs a live Zephyr env to iterate)

`<initializer_list>` is a *freestanding* C++ header (the compiler supplies it,
not the library); the true gap is Zephyr's `-nostdinc` not adding the GCC C++
include dir under minimal libcpp. Candidate fixes, in preference order:

1. Make the nros-cpp Zephyr build add the toolchain's **freestanding C++**
   include dir (`.../include/c++/<ver>`) without selecting full libstdc++ —
   gets `<initializer_list>` with no host-libc bleed. Best if achievable via the
   module's CMake/Kconfig.
2. Enable full libcpp **and** harden `platform.c`: include `<time.h>` explicitly
   for `CLOCK_MONOTONIC` and resolve the `timer_t` host/Zephyr collision (the
   hard part — likely needs the Zephyr POSIX `timer_t` to win the include order,
   or picolibc kept for the C TUs while libcpp serves only C++ TUs).
3. Make `parameter.hpp` freestanding-safe — provide the `std::initializer_list`
   ctor only when `<initializer_list>` is available, `#if __has_include(...)`.
   Keeps the 242.3 feature on hosted builds, drops it on bare freestanding.
   API-divergence cost; least preferred.

Cannot validate locally (no Zephyr SDK in the dev/agent sandbox; the multi-GB
`just zephyr setup` provision dies on detach) — CI-gated, and the lane is flaky
from concurrent main pushes cancelling runs (shared concurrency group). Found
2026-06-13; attempt-1 dead-end confirmed 2026-06-14. Cross-ref #42.
