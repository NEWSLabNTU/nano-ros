---
id: 892
title: "`nros build`'s west driver points at a directory that is not a west
  application, from a directory that is not a west workspace"
status: open
type: bug
area: build
related: [rfc-0065, phase-383]
filed: 2026-08-29
---

## Problem

`nros build <zephyr image>` emits a `west` command that cannot succeed. Found
while attempting phase-383 W9.b — retargeting `examples/fixtures.toml` rows at
`nros build` — which is blocked on it for all 14 remaining framework rows.

Two independent faults, each sufficient on its own.

### 1. The application dir is the BRINGUP, which has no `CMakeLists.txt`

```
$ nros build demo_bringup:zephyr --dry-run          # examples/workspaces/rust
nros build: demo_bringup:zephyr -> board native_sim/native/64 (platform zephyr), driver west
west build -b native_sim/native/64 …/examples/workspaces/rust/src/demo_bringup

$ ls examples/workspaces/rust/src/demo_bringup/
launch  package.xml  system.toml
```

west needs an application directory — a `CMakeLists.txt` that calls
`find_package(Zephyr)`. The bringup is a declaration package and has none. The
west application in this workspace is the hand-written entry:

```
$ ls examples/workspaces/rust/src/zephyr_entry/
boards  build.rs  Cargo.toml  CMakeLists.txt  package.xml  prj.conf  prj-cyclonedds.conf …
```

The cargo and cmake drivers both GENERATE their root (`builder/cargo_root.rs`,
`builder/cmake_root.rs`). There is no equivalent for west — `builder/zephyr.rs`
computes overlays and never emits an application.

### 2. The handoff runs west from the nros workspace, not a west workspace

`Handoff::new("west", a).in_dir(&root)` — `root` is the nros workspace:

```
$ cd examples/workspaces/rust && nros build demo_bringup:zephyr
west: unknown command "build"; do you need to run this inside a workspace?
```

The west workspace is `zephyr-workspace/`, which is where
`scripts/build/west-fixtures.sh` runs from.

## Why it went unnoticed

Nothing has ever built a zephyr image through `nros build`. Every zephyr fixture
row is still `entry = <hand-written package>` and goes through
`west-fixtures.sh`, which supplies the application dir, the build dir, the conf
fragments and the test locator itself. The driver is reachable only by asking
for an image nobody asks for.

phase-383 W5 ("Zephyr overlays and multi-image output") is marked done and its
overlay half IS implemented — `zephyr::west_args` emits `APPLICATION_CONFIG_DIR`
/ `EXTRA_CONF_FILE` / `EXTRA_DTC_OVERLAY_FILE`, and `[image.*]` carries `conf`
and `variant` for exactly this. What is missing is the application and the
working directory, so the overlays have nothing to overlay.

## What a migration needs beyond an application

The 14 rows carry three things `[image.*]` does not express:

| row field | what it does |
| --- | --- |
| `west_build_name` | the `-d` build directory, per fixture |
| `west_id`, `west_zenoh_locator` | runtime identity + locator baked into the image |
| `conf_files` | expressible today as the image's `conf` |

The first two are HARNESS concerns, not image ones — a fixture needs a private
build dir and locator so parallel legs do not collide. Whether they belong in
`[image.*]`, in the row, or in the handoff's native args is a design question
this issue does not settle.

## Directions

1. **Generate a west application** the way `cargo_root`/`cmake_root` generate
   theirs, from the image's `(launch, args, board)` plus its `conf`/`variant`.
2. **Run west from the west workspace**, resolved the way `west-fixtures.sh`
   does rather than assuming `root`.
3. Until both land, a zephyr fixture row cannot move off `entry =`.

## Reproduce

```sh
cd examples/workspaces/rust
NROS_SUPPRESS_DEPRECATION=1 nros build demo_bringup:zephyr --dry-run   # points at the bringup
NROS_SUPPRESS_DEPRECATION=1 nros build demo_bringup:zephyr             # west: unknown command "build"
```
