---
id: 325
title: "Tool-resolver residue after #219: an integrations shim uses HINTS (stale CLI shadows in-tree) and fails soft, a 5th bespoke nros resolver caches dead paths, and three idlc resolvers invert their own documented precedence"
status: open
type: bug
severity: high
area: build
related: [issue-0219]
---

## Finding (audit 2026-07-28, P1 + P2)

Issue #219 unified four `nros`-CLI resolvers under
`cmake/NanoRosCodegenCore.cmake`'s `nros_resolve_cli()`. Three resolver sites
outside that sweep survived, and they repeat the exact precedence mistake
CLAUDE.md:210 names verbatim ("`find_program` HINTS beat PATH — a stale
`~/.nros/bin` binary shadows the activate.sh CLI; use `PATHS` for fallbacks").

### P1 — `integrations/nano-ros/CMakeLists.txt:82`

```cmake
find_program(NROS_EXECUTABLE nros HINTS "$ENV{HOME}/.nros/bin")
```

HINTS are searched **before** PATH, so a stale provisioned CLI shadows the
`activate.sh`-wired in-tree one. Worse, the ESP-IDF `codegen-system` bake at
:84-100 then **fails soft** with `message(STATUS)`, so a museum CLI silently
produces a stale or absent baked system tree with no error — the "museum
binary passes the sweep" class, at configure time.

This is a NEW site: #219's four were `NanoRosEntry`, `nano_ros_workspace_metadata`,
zephyr `nros_system_generate`, and `_nros_resolve_codegen_tool`, all under
`cmake/` + `zephyr/`. `integrations/` was never swept.

Fix: `HINTS` → `PATHS`, or better
`include("${_nros_root}/cmake/NanoRosCodegenCore.cmake")` +
`nros_resolve_cli(NROS_EXECUTABLE CONTEXT "esp-idf shim")` — `_nros_root` is
already computed at line 16. And make the bake failure hard.

### P2 — `cmake/NanoRosBootstrapCodegen.cmake:43`

A fifth surviving bespoke resolver (called by
`cmake/platform/nano-ros-{freertos,nuttx,threadx}.cmake`). Precedence is
*correct* (`PATHS`), but it ignores `$ENV{NROS_CLI}`, carries its own divergent
FATAL text, and caches into `_path_codegen` — while the stale-path re-detect at
:33-41 only clears `_NANO_ROS_CODEGEN_TOOL`. Once the CLI moves, `if(_path_codegen)`
is still true from the previous configure and the module re-blesses a dead path.

Fix: replace the body with `nros_resolve_cli(_p CONTEXT "nros_bootstrap_codegen")`
and drop `_path_codegen`.

### P2 — three `idlc` resolvers, all with inverted precedence

`zephyr/cmake/nros_rmw_cyclonedds.cmake:264` and
`packages/dds/nros-rmw-cyclonedds/cmake/NrosRmwCycloneddsTypeSupport.cmake:61`
**and** `:116` (the same three-entry list copy-pasted twice inside one file).

All three put the retired in-tree `build/cyclonedds/bin` / `build/install/bin` in
`HINTS`, so they are searched before the host PATH — the opposite of what the
comment at `nros_rmw_cyclonedds.cmake:251-255` documents ("SDK store, host PATH
…, then the legacy in-tree build dirs … only a last-resort hint"). A stale
Phase-140-era in-tree `idlc` therefore shadows a fresh ROS 2 / SDK one, producing
type-support descriptors from a museum compiler — the
`find_descriptor() → nullptr` runtime-failure class.

Fix: keep only the SDK-store entries in `HINTS`; move the `build/...` entries to
`PATHS`. Collapse the two in-file copies into one `_nros_find_idlc()` helper and
have the Zephyr module include it.

## Why grouped

One root cause (bespoke `find_program` per call site, HINTS-vs-PATHS confusion),
one fix pattern (route through the shared resolver; `PATHS` for fallbacks), and
#219 already established both. Filing them apart would re-fragment the thing
#219 consolidated.
