---
id: 58
title: Zephyr C++ examples fail to build — `<initializer_list>` unavailable without full libcpp (regression from 242.3)
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

## Fix direction

Add `CONFIG_REQUIRES_FULL_LIBCPP=y` to the Zephyr cpp examples' base `prj.conf`
(the knob is overlay-independent; native_sim links the host libstdc++ so cost is
nil there). Apply to every `examples/zephyr/cpp/*/prj.conf`, not just the two in
the matrix, since all transitively include `parameter.hpp`.

Alternative considered (rejected): make `parameter.hpp` freestanding-safe by
dropping `<initializer_list>` — but 242.3 deliberately needs it for the
`initializer_list` ctors; reverting loses the feature. The header is correct;
the example Kconfig is the gap.

Cannot validate locally (no Zephyr SDK in the dev/agent sandbox; the multi-GB
`just zephyr setup` provision dies on detach) — CI-gated. Found 2026-06-13 while
triaging the chronically-red zephyr-dual-line lane.
