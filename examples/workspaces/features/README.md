# `features` — the capability demos, one workspace

The per-feature micro-workspaces
(`ws-qos-*`, `ws-params-*`, `ws-lifecycle-*`, `ws-custom-msg-*`,
`ws-remap-*`) were folded into this one. Feature coverage used to be expressed as
DIRECTORIES — one workspace per feature × language — which multiplied build
cost without multiplying what was actually covered.

Here the axis is the **launch file**, not the directory: `demo_bringup/launch/`
carries one entry per (capability × language), and each resolves to its own
model.

## Layout

```
src/
  demo_bringup/          the bringup — one launch file per capability × language
    boards/<board>/      the Zephyr images' Kconfig
  <lang>_<feature>_pkg/  the node packages (49 of them)
```

There are no entry packages here: `nros build <image>` GENERATES
the whole west application — `CMakeLists.txt`, `build.rs`, the staticlib
manifest and `src/lib.rs` — under `build/<coord>/<image>_entry/`, so the three
`[image.zephyr_rust_*]` rows in `demo_bringup/system.toml` are now the only
place those images are described. Their Kconfig, which is not derivable, moved
to the shared `demo_bringup/boards/native_sim_native_64/` — the three copies
were byte-identical.

Capabilities covered: QoS overrides, parameters, lifecycle (managed nodes),
custom messages, topic remapping, and `reading_*` (a subscriber reading a
declared parameter).

Languages: C, C++ and Rust node packages sit side by side, so the language seam
is exercised inside a single workspace rather than across four copies of one.

## Building

```sh
nros configure
nros build                       # every [image.*]
nros build linux_rust_qos       # or one capability at a time
```

Each image names its own board, so each is built into its own `build/<coord>/`;
the Zephyr images keep `west build` as their verb, with `nros configure` before it.

A capability's model comes from its launch file — `nros configure` resolves one per
entry. `[[model]]` declarations in `demo_bringup/system.toml` cover the variants
that take launch ARGUMENTS, which cannot be derived from the launch tree alone.

## Why not one workspace per feature

Bisection gets coarser: a QoS regression now fails inside a workspace that also
builds params and lifecycle, and one broken node package blocks the whole
fixture. That trade is accepted deliberately, and revisited if the pain
turns out to be real.
