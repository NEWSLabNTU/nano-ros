---
id: 3011
title: "A nested entry's census and probe scan its parent directory, not the workspace, and miss its dependencies"
status: open
type: bug
severity: medium
area: [orchestration, cli]
related: [2899, 2680, 2862, phase-478]
found: 2026-11-27
---

**Found by** NXP 175 on the ASI workspace (CI run 46002985809, both FreeRTOS
lanes), checked at `0e4afce2fa`. A follow-up to
[issue 2899](archived/2899-owned-census-omits-entry-workspace-depends.md),
whose fix (`8f253ba230`) is correct for a package directly under `src/` and
wrong for one nested below it.

## What happens

`probe_deps::workspace_deps(package_dir)` scanned `package_dir.parent()` for
the workspace packages, assuming every package sits at `<ws>/src/<pkg>`.
ASI's FreeRTOS test entry is at `src/asi_tests/asi_unit_test`, so the scan
read `src/asi_tests`, found no `asi_common`, and returned an empty closure.
The census include then held only `enable_language(C)`, and the census
build failed:

```
unit_test.cpp:23: fatal error: common/clock/clock.hpp: No such file or directory
```

reported from `NanoRosLeafEntityFacts.cmake:324`. colcon finds packages at
any depth under `src/`, so the nesting is legitimate.

The same assumption sat in a second place: the C/C++ metadata probe set
`NROS_INTERFACE_SEARCH_PATH` to the first component's parent directory, so a
nested component searched only its own subdirectory for interface packages.

## Fix

One helper, `probe_deps::workspace_src(package_dir)`, answers "which `src/`
is this package's workspace" with `model_location::workspace_root`, the one
rule that places a package in a workspace (issue 2862): the nearest
`.colcon_workspace` whose `src/` holds it, else the nearest `src/` outside a
package. The dependency scan (and so the census closure and the probe's
freshness key) and the probe's interface search path both read it. A package
in no workspace keeps its parent directory, the answer it had before.

Sweep: `git grep -n 'package_dir.parent()\|package_root.parent()' --
packages/cli` found the two sites above and no others before the fix;
after it, only the helper's own no-workspace fallback.

## Test

`a_nested_package_finds_dependencies_anywhere_under_src` in
`probe_deps.rs`: an entry at `src/asi_tests/asi_unit_test` depending on
`src/asi_common` resolves its `src/` to the workspace's, and its closure is
`asi_common`.
