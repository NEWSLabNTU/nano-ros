---
id: 1962
title: "The two bridge workspaces' generated `linux_entry` does not compile: `nros::main!` finds no `__nros_register_backends` / `__nros_register_type` in its selection crate"
status: resolved
resolved: 2026-10-19
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
  crate (`linux_entry_nros_selection`) to export.

## Cause

The generator was fine. The selection crate on disk was not the generator's
output: `generated/nros-selection/linux_entry/src/lib.rs` in both workspaces
was dated 2026-10-15 15:09 and read "This crate is intentionally empty" — the
shape from before RFC-0071 D11 (phase-506 W2) moved backend registration into
the facade as `__nros_register_backends!` / `__nros_register_type!`.

Nothing rewrote it, by construction:

- `nros build` (`generate_entry`) wrote a facade only through its HEAL path,
  which runs when the directory is MISSING. A facade that existed was taken
  as-is, whatever generator had written it.
- `nros sync` writes a facade per Rust package it can see. A generated entry
  under `build/<coordinate>/` is not one of them, so sync never touched it
  either (measured: `nros sync --verbose` in `bridge-cyclonedds` printed no
  facade line and left the file unchanged).

So a facade was written once, by the first build that found none, and then
frozen. Every other workspace carries the same frozen facades (all ten in
`examples/workspaces/rust` lacked the macros); only the bridge and Zephyr
entries expand them, which is why only the bridge rows broke on the linux
lane. A fresh clone never sees this, because there the heal writes a current
facade — it is a defect of incremental trees, i.e. of every developer host
and every persistent runner workspace.

## Fix

`generate_entry` now rewrites an EXISTING facade on every build, through the
same `write_entry_facade` helper the heal uses. `write_facade` writes only what
changed, so an up-to-date tree pays nothing, and the entry already depends on
that directory, so it needs no second write. Unit test:
`a_stale_facade_is_rewritten_by_the_current_generator`.

## Acceptance

Both rows build with the post-1961 fixture builder, and the tests that consume
them (`bridge_zenoh_to_cyclonedds`, `declarative_bridge_zenoh_to_*`) pass on the
rebuilt binaries.
