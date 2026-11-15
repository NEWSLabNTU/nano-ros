---
id: 2609
title: "A generated Zephyr app loses its workspace Zephyr modules when `nros build` passes `-DZEPHYR_EXTRA_MODULES`"
status: resolved
type: bug
area: [cli, zephyr]
severity: medium
found: 2026-11-15
resolved: 2026-11-15
resolved_in: "the change that filed it"
related: [phase-478, issue-2594]
---

GitHub: NEWSLabNTU/nano-ros#3948

**Home: [phase-478](../../roadmap/phase-478-island-developer-experience-findings.md), I15.**
Reported by the Autoware Safety Island on nano-ros a5e0439885.

## What happens

Issue 2594 made a workspace package that carries `zephyr/module.yml` join
the generated west application's modules. The application appended it to a
normal variable before `find_package(Zephyr)`:

```cmake
list(APPEND ZEPHYR_EXTRA_MODULES "${_nros_zephyr_module_0}")
```

`nros build` passes the nano-ros module on the `west build` line as a CACHE
entry (`-DZEPHYR_EXTRA_MODULES=<nano-ros>`, `builder::zephyr::module_arg`).
Zephyr 3.7 reads the list with `zephyr_get(... ZEPHYR_EXTRA_MODULES)` without
`MERGE`, which returns the first scope that defines it, and the cache comes
before the current scope. So the cache value (nano-ros alone) won, and the
workspace module never reached Kconfig: `prj-fvp.conf` setting one of its
symbols was refused as undefined. A hand-written app configured without the
`-D` worked, which is why the unit tests (which rendered the file and checked
the line's position) passed.

## Resolution (2026-11-15)

The generated application now adds its modules to the CACHE entry it was
given, keeping what it held:

```cmake
set(_nros_zephyr_modules ${ZEPHYR_EXTRA_MODULES})
list(APPEND _nros_zephyr_modules "<module>")
list(REMOVE_DUPLICATES _nros_zephyr_modules)
set(ZEPHYR_EXTRA_MODULES "${_nros_zephyr_modules}" CACHE STRING "..." FORCE)
```

Checked with a throwaway CMake project configured with
`-DZEPHYR_EXTRA_MODULES=<x>`: the cache entry is `<x>;<module>`, unchanged on
a second configure and on one without the `-D`. Test:
`a_zephyr_module_package_joins_the_extra_modules_before_find_package`, which
now refuses the old `list(APPEND ZEPHYR_EXTRA_MODULES ...)` line.
