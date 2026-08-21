---
id: 754
title: "Zephyr module's idlc store lookup re-finds `nros` on PATH instead of
  reusing the resolved codegen tool — consumers must export PATH themselves"
status: open
type: tech-debt
area: cmake, zephyr, rmw
related: [issue-0663, issue-0625, phase-246]
---

## Problem

`zephyr/cmake/nros_rmw_cyclonedds.cmake` resolves the SDK-store cyclonedds
prefix (for host `idlc`) by finding the CLI again from scratch:

```cmake
find_program(_NROS_CLI_IDLC nros)
if(_NROS_CLI_IDLC)
    execute_process(COMMAND "${_NROS_CLI_IDLC}" sdk-path cyclonedds ...)
```

— even though the build has, by this point, already resolved a validated
CLI into `_NROS_ZEPHYR_CODEGEN_TOOL` (west `-D` pre-set → Kconfig
`CONFIG_NROS_CODEGEN_TOOL` → `_nros_resolve_codegen_tool()`, Phase 246.2b).
The canonical lane likewise passes `-D_NANO_ROS_CODEGEN_TOOL=<path>`.

So a consumer that hands the build an explicit CLI still needs `nros` ON
PATH for the idlc store rung to work. If PATH has no `nros`, the store
lookup silently degrades to the host-PATH idlc search; if PATH has a
DIFFERENT `nros` (older checkout, stale `~/.nros/bin` copy — the
issue-0663/0625 shadowing class), the store answer can disagree with the
tool the build was told to use.

## Evidence (consumer side)

autoware-safety-island's `build.sh` Zephyr lane passes
`-D_NANO_ROS_CODEGEN_TOOL="${nros_cli_bin}"` AND must additionally
`export PATH="$(dirname "${nros_cli_bin}"):${PATH}"` — the export exists
only because a clean reconfigure exposed this second, PATH-based discovery
(found during ASI phase-5 W1, 2026-08-22).

## Direction

Reuse the tool the build already validated: try
`_NROS_ZEPHYR_CODEGEN_TOOL` / `_NANO_ROS_CODEGEN_TOOL` first and fall back
to `find_program(nros)` only when neither is set. One resolution, one
answer — the same single-resolver rule the codegen tool itself follows.
