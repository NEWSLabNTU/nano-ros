---
id: 1962
title: "The two bridge workspaces' generated `linux_entry` does not compile: `nros::main!` finds no `__nros_register_backends` / `__nros_register_type` in its selection crate"
status: open
type: bug
area: [build, cli, rmw]
severity: medium
found: 2026-10-19
related: [1961]
---

## What happens

`workspace-rust-native-bridge` (`examples/workspaces/bridge-cyclonedds`) and
`workspace-rust-native-bridge-xrce` (`examples/workspaces/bridge-xrce`) both fail
in `nros build demo_bringup:linux`, compiling the generated entry:

```
   Compiling linux_entry v0.0.0 (.../examples/workspaces/bridge-cyclonedds/build/posix-zenoh/linux_entry)
error[E0433]: cannot find `__nros_register_backends` in `linux_entry_nros_selection`
  --> src/main.rs:9:1
 9 | / nros::main!(
10 | |     launch = "demo_bringup",
11 | |     spin = "forever",
12 | | );
error[E0433]: cannot find `__nros_register_type` in `linux_entry_nros_selection`
```

`nros build` exits 101.

## How long, and why nobody saw it

The fixture binaries on this host date from **2026-10-15**
(`target-fixtures/nros-relwithdebinfo/linux_entry`). Every `linux` lane since
then reported both rows as `built:` over that binary, because the fixture
builder ran each row with errexit off (issue 1961). Measured with the
pre-1961 script on 2026-10-19: the same E0433, then `built:` and exit 0. With
1961's fix both rows fail, which is how this was found.

So anything that consumed these fixtures since then ran a museum binary.

## What is known

- Both workspaces are bridge systems: `[system] rmw = "zenoh"` plus `[[domain]]`
  / `[[bridge]]` blocks (`src/demo_bringup/system.toml`). The fixture rows'
  `rmw` (`cyclonedds`, `xrce`) is the bridged side; the entry builds under
  `build/posix-zenoh/`.
- The failing symbols are the two the macro expects the generated selection
  crate (`linux_entry_nros_selection`) to export. Not investigated further:
  which change since 2026-10-15 stopped the selection crate emitting them for a
  bridge system, or whether it is the multi-RMW selection that lacks them.

## Acceptance

Both rows build with the post-1961 fixture builder, and the tests that consume
them (`bridge_zenoh_to_cyclonedds`, `declarative_bridge_zenoh_to_*`) pass on the
rebuilt binaries.
