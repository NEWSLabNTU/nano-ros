# `features` — the capability demos, one workspace

Phase-331 W2 (RFC-0066) folded the per-feature micro-workspaces
(`ws-qos-*`, `ws-params-*`, `ws-lifecycle-*`, `ws-custom-msg-*`,
`ws-remap-*`) into this one. Feature coverage used to be expressed as
DIRECTORIES — one workspace per feature × language — which multiplied build
cost without multiplying what was actually covered.

Here the axis is the **launch file**, not the directory: `demo_bringup/launch/`
carries one entry per (capability × language), and each resolves to its own
model.

## Layout

```
src/
  demo_bringup/          the bringup — one launch file per capability × language
  <lang>_<feature>_pkg/  the node packages (49 of them)
  zephyr_rust_*_entry/   per-capability Zephyr entries
```

Capabilities covered: QoS overrides, parameters, lifecycle (managed nodes),
custom messages, topic remapping, and `reading_*` (a subscriber reading a
declared parameter).

Languages: C, C++ and Rust node packages sit side by side, so the language seam
is exercised inside a single workspace rather than across four copies of one.

## Building

The workspace builds ONE platform per configure, like every other workspace
here (RFC-0026):

```sh
cmake -S . -B build -DNANO_ROS_PLATFORM=posix
cmake --build build
```

A capability's model comes from its launch file — `nros sync` resolves one per
entry. `[[model]]` declarations in `demo_bringup/system.toml` cover the variants
that take launch ARGUMENTS, which cannot be derived from the launch tree alone
(phase-330 W4.0).

## Why not one workspace per feature

Bisection gets coarser: a QoS regression now fails inside a workspace that also
builds params and lifecycle, and one broken node package blocks the whole
fixture. RFC-0066 accepts that trade deliberately, and phase-331 W5 is the
checkpoint where it is revisited if the pain turns out to be real.
