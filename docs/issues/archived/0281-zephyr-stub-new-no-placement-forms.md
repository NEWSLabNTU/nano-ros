---
id: 281
title: "Zephyr minimal-libcpp stub <new> lacked placement forms — every NROS_COMPONENT factory failed to compile on native_sim"
status: resolved
type: bug
severity: medium
area: nros-cpp
---

## Finding (autoware-safety-island-example ports, 2026-07-24 — porting-notes 18)

Zephyr's minimal libcpp stub `<new>` has no placement-new declarations, and
GLIBCXX full libcpp is unreachable on native_sim host-gcc
(`PICOLIBC_USE_MODULE depends on !GLIBCXX_LIBCPP`; host gcc ships no
toolchain picolibc). Every `NROS_COMPONENT` factory uses placement new →
whole-image compile failure. The ASI FVP build never hit this
(full-libcpp cross toolchain), so it surfaced only on native_sim.

## Resolution (same-day, 2026-07-24)

`component_node.hpp` declares the non-allocating forms when the Zephyr stub
guard is present. Related environment fact: `<algorithm>`/`<cmath>` are
also absent — ports carry local `max_d`/`abs_d`. Filed retroactively for
the record trail.
