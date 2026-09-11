---
id: 1288
title: "The eight Rust Zephyr workspace entries are still hand-written — `nros build` has no generator for a west application"
status: open
type: tech-debt
area: tooling, examples, zephyr
severity: medium
found: 2026-09-11
related: [1108, 1253, rfc-0065, rfc-0098, phase-445]
---

# What is left of RFC-0065 D4 on Zephyr

phase-445 W5 turned every hand-written CARGO workspace entry into a generated
one (`examples/workspaces/rust/src/esp32_entry` was the last) and moved every
workspace entry's deployment out of its manifest into the bringup image that
claims it (`leaf_system::for_entry`). Eight entries are still hand-written
packages, all Rust west applications:

| workspace | entry | image |
| --- | --- | --- |
| `rust` | `zephyr_entry` | `[image.zephyr]` |
| `rust` | `zephyr_entry_robot1` | `[image.zephyr_robot1]` |
| `realtime-rust` | `zephyr_entry` | `[image.zephyr]` |
| `features` | `zephyr_rust_{lifecycle,params,qos}_entry` | `[image.zephyr_rust_*]` |
| `safety` | `zephyr_rust_safety_entry` | `[image.zephyr_rust_safety]` |

(The C/C++ `zephyr_entry` / `fvp_entry` packages are the same shape and the
same gap.)

## Why they were not generated

`builder::entry` generates a CARGO entry (`build/<coord>/<id>_entry/`), and
for a `ZephyrStaticlib` board it already renders the `rustapp` staticlib. What
it cannot produce is the WEST APPLICATION around it — `CMakeLists.txt` with
`find_package(Zephyr)` and `rust_cargo_application()`, `prj.conf`,
`prj-<rmw>.conf`, `boards/*.overlay`, `sample.yaml`, `build.rs` with
`zephyr-build`. RFC-0065 D4 puts the overlays in the BRINGUP
(`boards/<board>/`) and D5 calls authored Kconfig "not derivable", and
`west_application_dir` (`cmd/build.rs`) says the same in code: "Inventing an
application here would be worse". So the entry is derivable and the application
that hosts it is not, and nothing yet splits the two.

## What W5 did to them

- Deployment facts moved to the bringup image (`locator` per image; board and
  RMW were already there). The manifests keep only the empty
  `[package.metadata.nros.entry]` marker. `nros::main!`, `nros sync`'s board
  projection, `nros ws board-facts` (the Zephyr lane's board facts, via
  `NanoRosBoardFacts.cmake`) and `nros build`'s entry classification all read
  it through `leaf_system::for_entry`.
- `examples/workspaces/rust`'s two images keep `board = "native_sim/native/64"`;
  the macro's board table gained that key (it is the zephyr descriptor's second
  name) so the image's board resolves as the hand-written `deploy = "zephyr"`
  did.
- **None of it was BUILT.** The west workspace's `nano-ros` module is a symlink
  to another checkout (issue 1253's shape), so a west build from the phase-445
  worktree compiles that checkout, not this one. The acceptance for this issue
  is a west build of each of the eight.

## Fix

Generate the application too: move each entry's `prj*.conf` / `boards/` into
`<bringup>/boards/<board>/` (D4's table), and have stage 4 emit the west
application shell (`CMakeLists.txt` + `sample.yaml` + `build.rs`) around the
generated staticlib entry under `build/<coord>/`, pointing `west build` there
with `APPLICATION_CONFIG_DIR` at the bringup's board dir (`builder::zephyr`
already resolves those overlays). Then delete the eight packages.
