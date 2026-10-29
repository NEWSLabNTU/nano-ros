---
id: 2152
title: "A Zephyr entry from `nros new entry` never gets its selection package, so it does not compile outside a checkout"
status: open
type: bug
area: [cli, scaffold, zephyr]
severity: medium
found: 2026-10-30
related: [2151, phase-517]
---

## What

`nros new entry zephyr_entry --platform zephyr` writes an entry whose
`src/lib.rs` is `nros::main!(launch = "demo_bringup")`. That macro takes the
image's backend registration from the generated selection package,
`zephyr_entry_nros_selection` (RFC-0071 D11). Nothing ever creates that
package or adds the dependency on it, so the entry fails to compile:

```text
error: nros::main!: this Zephyr entry's backend registration comes from the
image's generated selection package (`<package>_nros_selection`, RFC-0071 D11),
and this entry's Cargo.toml does not depend on it. Run `nros sync` (or
`nros build`), which writes the package and the dependency.
```

The remedy it names does not work. This was measured on phase-517 W17's
`zephyr-rust` installed probe track (`probe-platforms` run 40921005534):
`nros sync` ran before the build, and the build still failed with this
error. It reproduces locally from a scaffold. After `nros sync -v` there is
no `generated/nros-selection/` directory and no facade line in the output.

## Why

`nros sync` writes selection facades in `ws::generate_facade_crates`, which
calls `facade::write_facade` for each Rust package an image claims. The
scaffolded entry is claimed (`[image.zephyr_entry] entry = "zephyr_entry"`),
but `write_facade` returns early:

* It needs the entry's `nros` path. `facade::runtime_deps` reads only
  **`path`** dependencies, so `deps.nros` is `None` and no facade is written.
* The scaffold writes **registry-style** dependencies:
  `nros = { version = "*" }` and `nros-board-zephyr = { version = "*" }`.
  These resolve through the central `[patch.crates-io]` rows, which is the
  shape every scaffold uses.
* Even with a facade written, nothing adds
  `zephyr_entry_nros_selection = { path = … }` to the entry's `Cargo.toml`.
  Generated entries get the dependency from the generator, and a
  hand-written entry has no such writer.

In-tree Zephyr Rust images use `nros::zephyr_component_main!` with their own
features, so no in-tree lane exercises this scaffold. Its only test plans it
(`--dry-run`) and never compiles it.

## Fix direction

* `facade::runtime_deps`: resolve a registry-style `nros` / `nros-board-*` /
  `nros-rmw-*` dependency through the same patch table `nros sync` writes,
  or through the SDK root, instead of skipping it.
* Have `nros new entry` write the
  `<pkg>_nros_selection = { path = "<facade dir>" }` dependency, or have
  sync add it the way the error message promises.
* Add a fixture that builds the scaffolded entry, so a scaffold that only
  plans can no longer pass.

Until then, phase-517 W17's `zephyr-rust` probe track stays out of the
matrix. Its earlier runs also found issue 2151 and the native_sim
`CONFIG_NET_DRIVERS` fix.
