---
id: 2652
title: "A build dir keeps the cached `nros` codegen tool after a different `nros build` runs into it"
status: resolved
type: bug
area: [cmake, build, cli]
severity: medium
found: 2026-11-16
resolved: 2026-11-16
resolved_in: "the change that filed it"
related: [phase-478, issue-1880, issue-1360]
---

**Home: [phase-478](../../roadmap/phase-478-island-developer-experience-findings.md), I24.**
GitHub [#4067](https://github.com/NEWSLabNTU/nano-ros/issues/4067).
Reported by the Autoware Safety Island on nano-ros 09219edc75 (freertos-posix
and s32z images).

## What happened

1. An image was built with the in-tree CLI. CMakeCache.txt recorded
   `_NANO_ROS_CODEGEN_TOOL:INTERNAL=<checkout>/packages/cli/target/release/nros`.
2. A different (release) `nros build <image>` ran into the same build dir.
3. `nros_bootstrap_codegen()` honored the cached tool as-is because the file
   still existed, so `nros dev rmw dispatch --known` ran the old in-tree
   binary, and its stale guard failed the configure ("in-tree nros CLI is
   STALE", NanoRosRmwDispatch.cmake:331).

The island worked around it by passing `-D_NANO_ROS_CODEGEN_TOOL=<selected
cli>` on every configure.

Issue 1880's rung (the checkout's own CLI ahead of the cache) did not help:
it fires only when the cmake modules live in a checkout, and the release
road's SDK root has no `packages/cli/target`.

## Resolution (2026-11-16)

The invoking binary is the truth.

- `nros build` passes `-D_NANO_ROS_CODEGEN_TOOL=<its own absolute path>`
  (`builder::codegen_tool_arg`) on every CMake configure it starts: the cmake
  image road, the cmake package roads, the west road after `--` (the Zephyr
  lane adopts it into `_NROS_ZEPHYR_CODEGEN_TOOL`), and the census build of
  `nros configure --census`. It comes before `--cmake-args` and the native
  args, so a caller's own `-D_NANO_ROS_CODEGEN_TOOL` still wins. The configure
  stamp and the west configure digest both cover it, so switching CLIs
  re-configures and a repeat with the same CLI does not. The NuttX kernel
  step is a bash script that starts no nano-ros configure; NuttX images reach
  CMake through the cmake road above. The metadata probe already passed it.
- A bare `cmake` re-configure has no invoker, so the cache is kept. But
  `nros_bootstrap_codegen()` now asks a cached tool for its
  `--codegen-version` and refuses at once, naming the tool, its version, the
  accepted range and the tree, when this tree's runtime does not accept it.
  `_nros_rmw_cli` goes through the bootstrap even when a tool is cached, so
  the refusal comes before the dispatch query runs the tool. A tool that
  cannot answer, or a tree with no `codegen_version.rs`, is not refused.

Tests: `every_configure_names_the_invoking_nros_as_the_codegen_tool`
(`build_verb_pipeline.rs`: cmake and west roads, caller override wins) and
`just check codegen-tool-cache` (`tests/cmake-codegen-tool-cache-tests.sh`:
an invoker's `-D` replaces the cached tool, a bare re-configure keeps a usable
one, refuses an out-of-range one, and does not refuse a tool with no answer).

A same-version stale in-tree binary under a bare `cmake` re-configure still
fails at the stale guard, whose message names the fix; the codegen version
cannot tell those two binaries apart.
