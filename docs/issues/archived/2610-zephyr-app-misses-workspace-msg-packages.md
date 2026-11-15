---
id: 2610
title: "A generated Zephyr app does not find the workspace's own message packages"
status: resolved
type: bug
area: [cli, zephyr, codegen]
severity: medium
found: 2026-11-15
resolved: 2026-11-15
resolved_in: "the change that filed it"
related: [phase-478, issue-2594]
---

GitHub: NEWSLabNTU/nano-ros#3952

**Home: [phase-478](../../roadmap/phase-478-island-developer-experience-findings.md), I16.**
Reported by the Autoware Safety Island on nano-ros a5e0439885.

## What happens

A workspace interface package (`rosidl_generate_interfaces`) is deliberately
not a subdirectory of a generated application (issue 0862). The CMake root
therefore puts the workspace's `src/` on `NROS_INTERFACE_SEARCH_PATH`
(issue 1111), and the generated Zephyr west app did not. Its configure found
none of the workspace's message packages, registered fewer message-bound
tables than `nros configure` had composed, and the promise check
(`NanoRosPromise.cmake`) refused the build, naming the two missing packages.
`NROS_INTERFACE_SEARCH_PATH=<workspace>/src nros build fvp` was the
workaround.

## Resolution (2026-11-15)

The generated Zephyr app emits the same block as the CMake root, before
`find_package(Zephyr)`: the workspace's `src/`, relative to the application,
prepended to `NROS_INTERFACE_SEARCH_PATH` so an exported entry keeps its
precedence. The block now has one rendering,
`cmake_root::interface_search_path_cmake`, which both generators call; the
CMake root's output is unchanged. Test:
`the_workspace_interface_packages_are_on_the_search_path`.
