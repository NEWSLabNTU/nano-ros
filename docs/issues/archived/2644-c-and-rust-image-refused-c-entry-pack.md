---
id: 2644
title: "An image of C and Rust components with no C++ source is refused at configure: the C entry pack cannot construct a Rust component"
status: resolved
type: bug
area: [cli, codegen, cmake]
severity: medium
found: 2026-11-16
resolved: 2026-11-16
resolved_in: "the change that filed it"
related: [phase-567]
---

**Home: [phase-567](../../roadmap/archived/phase-567-regression-prevention.md) W4.** Found
while building the island-shaped workspace (`examples/workspaces/island-shaped`),
whose image holds a C component and a Rust component and no C++ source.

## Symptom

`nros configure --census island_bringup:freertos` (and `nros build` of any image
of the workspace on the CMake road) stopped in the generated entry:

```
Error: typed entry emit (`c` pack): node pkg `rust_monitor_pkg` exec
`monitor` is lang `rust` (a `rust` component), not `c` -- the `c` entry
pack constructs `c` components only (use `--lang cpp` for a `rust`
component)
```

## Cause

The generated entry's `LANG` was `cpp` only when a package held a C++ source
(`discover::holds_cpp_source`), on the cmake root (`cmd::build`, two sites) and
on the generated west application (`west_app::resolve_cmake`). A Rust component
needs the C++ pack, and `examples/workspaces/mixed`, the one in-tree image with a
Rust component on these roads, also has C++ components, so nothing exercised C +
Rust alone. On the west road the scan also skipped a Rust package with no
`CMakeLists.txt` before asking.

## Fix

`discover::needs_cpp_entry`: a C++ source, or a Rust node package (a ROS package
whose `Cargo.toml` declares `[package.metadata.nros.node]`). All three sites ask
it; the west application also takes `cpp` when it derives a Rust node's
registration.

A second half showed once the image configured: the message-bound seed read the
probe's bound tables for the ENTRY's language (`nano_ros_cpp`), and the probe
generates per COMPONENT language, so the only tables were the C component's
(`nano_ros_c`). Every build said "message bounds not seeded: the metadata probe
generated no bound table for `builtin_interfaces`", and on Zephyr
check-knob-delivery then refused the build dir (three message-bound knobs
derived after configure and never resolved). `cmd::build::probe_bound_tables`
takes the entry language's tables when they exist and the other language's
otherwise (a table prices a type, so either is the same bound), the rule the
hand-written-entry site already used. Tests: `discover::tests::a_rust_node_package_needs_the_cpp_entry`,
`west_app::tests::lang_is_cpp_when_a_rust_node_joins_c_nodes`,
`probe_bound_tables_tests::the_other_languages_tables_are_taken_when_the_entrys_are_absent`. The island-shaped
workspace keeps the layout in the merge queue (`just check
island-shaped-workspace`).
