---
id: 326
title: "Zephyr detection guards keyed on the possibly-unset NANO_ROS_PLATFORM at 5 more sites — #282 fixed one of six, and the fix introduced a second idiom"
status: open
type: bug
severity: medium
area: build
related: [issue-0282, issue-0328]
---

> **Shared pattern with #328 — "the fix landed at the site, not the class."**
> #282 fixed 1 of 6 identical guards; #222 fixed 4 of ~34 identical fixture
> resolvers (→ #328). Same failure mode, different subsystem: a grep-able class
> gets fixed where the symptom appeared, so the remaining siblings stay armed and
> the next incident looks new. Whoever picks up either issue should sweep the
> whole class and land ONE shared helper — see the "Fix the CLASS" practice in
> `CLAUDE.md`, and the audit note in
> `docs/development/audit-findings-2026-07-28.md`.

## Finding (audit 2026-07-28, P2)

`5a8db2413` (issue #282) fixed one instance of the unset-variable guard class at
`cmake/NanoRosVerbs.cmake:290`. Five sibling sites still carry the defect:

- `cmake/NanoRosNodeRegister.cmake:252` — OBJECT_DEPENDS / `app`-include-mirror ordering
- `cmake/NanoRosNodeRegister.cmake:418` — interface-lib skip (**inverted**: wrongly
  links non-target `-lstd_msgs__nano_ros_cpp` names)
- `cmake/NanoRosNodeRegister.cmake:436` — mirror ordering
- `cmake/NanoRosNodeRegister.cmake:855` — the Zephyr carrier
- `cmake/NanoRosEntry.cmake:226` — `_nra_is_zephyr`

## Mechanism

`nano_rosConfig.cmake:41` sets `NANO_ROS_PLATFORM zephyr` as a **plain,
directory-scoped** variable in whatever scope calls `find_package(nano_ros)`.
Every `add_subdirectory`'d node/component package that does not itself call
`find_package` therefore evaluates

```cmake
if(NANO_ROS_PLATFORM STREQUAL "zephyr")
```

with the variable UNSET — cmake compares the literal string
`"NANO_ROS_PLATFORM"` against `"zephyr"` → FALSE, and the branch silently takes
the wrong path (`if(X)` truthiness is the safe idiom; see checklist A2).

`docs/issues/archived/0282-*.md` records this residual itself: "the legacy fused
`nano_ros_node_register` Zephyr branch carries the same unset-variable guard at
three sites … passes only by timing luck."

Severity is P2 rather than P1 only because no in-tree example currently exercises
the fused path or a Zephyr `nano_ros_add_executable` — verified: no tracked
`nano_ros_add_executable` under `examples/zephyr/**`. It is a latent trap for the
next Zephyr consumer, not a live break.

## Also: the fix became A1 drift

`5a8db2413` introduced a *second* detection idiom at the one site it fixed rather
than a shared helper, so the codebase now has two spellings of "am I Zephyr?"
across six sites.

## Fix

Add one helper to `cmake/NanoRosCodegenCore.cmake`:

```cmake
function(_nros_is_zephyr out)
    if(TARGET app AND (DEFINED ZEPHYR_BASE OR NANO_ROS_PLATFORM STREQUAL "zephyr"))
        set(${out} TRUE PARENT_SCOPE)
    else()
        set(${out} FALSE PARENT_SCOPE)
    endif()
endfunction()
```

and call it from all six sites (including the one already fixed, to collapse the
two idioms). Alternatively promote `NANO_ROS_PLATFORM` to `CACHE INTERNAL` in
`nano_rosConfig.cmake`'s Zephyr arm so subdirectory scopes inherit it — but the
helper is preferable because it also covers the `ZEPHYR_BASE`-only case.

## Not findings (checked)

`cmake/NanoRosGenerateInterfaces.cmake:181,869` have the same shape but are
inert: the native generator is not loaded in a Zephyr build
(`nano_rosConfig.cmake`'s Zephyr arm returns at :58 before including it). Their
comments describe a Zephyr scenario they cannot serve — worth a note, not a fix.
All 39 `STREQUAL ""` hits elsewhere in `cmake/` were read and killed (each is
short-circuited by `DEFINED`, quoted-deref, a foreach variable, a function
parameter, or a preceding `set()` — see the audit findings doc).
