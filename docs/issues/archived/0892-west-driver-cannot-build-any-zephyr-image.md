---
id: 892
title: "`nros build`'s west driver points at a directory that is not a west
  application, from a directory that is not a west workspace"
status: resolved
type: bug
area: build
related: [rfc-0065, phase-383]
filed: 2026-08-29
resolved: 2026-08-29
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

## Fixed — the driver now matches Zephyr's actual model

The framing that settled it: **Zephyr is not like the other drivers, and the
handoff has to say so.** cargo and cmake let `nros build` OWN the root — it
generates one from the images and hands the tool a directory it wrote. west does
not work that way. The user owns the west workspace (`west init` / `west
update`), the application is a stock Zephyr app whose `prj.conf` and
`CMakeLists.txt` carry authored Kconfig no image declaration expresses
(RFC-0065 D5), and west only runs inside that workspace.

So the fix is NOT to generate an application. It is to stop pretending:

1. **Point at the real application.** `west_application_dir` finds the framework
   entry package whose `[package.metadata.nros.entry] deploy` resolves to the
   image's board — the same resolution `framework_entry_dirs` already uses, so a
   workspace with several zephyr entries picks the right one. Identity is the
   descriptor's NAME SET, since one board has several spellings.
2. **Find the user's workspace, never assume one.** `$NROS_WEST_WORKSPACE` →
   nearest `.west/` above the CWD, then above the build root → `$ZEPHYR_BASE`'s
   parent. CWD first because that is how Zephyr is actually used: the user
   stands in their workspace and builds.
3. **When there is none, print the command instead of emitting a broken one.**
   Same boundary `nros setup --system` draws with `sudo`: compose it, hand it
   over, do not act.

```
$ nros build demo_bringup:zephyr
Error: no west workspace found, so `west build` cannot be run for `zephyr`.
…
Run this from your west workspace:

    west build -b native_sim/native/64 …/src/zephyr_entry

Or point nros at it:  NROS_WEST_WORKSPACE=<dir>
```

**Resolved at plan time, ENFORCED at exec.** `--dry-run` must answer "what would
you run" from a machine with no west workspace at all — refusing there withholds
the very command the message tells the user to run. An existing pipeline test
caught that: it asserts driver selection for a zephyr image in a temp dir, and
needs no workspace to do so.

All three rungs verified by running them: explicit pointer, upward search from
inside `zephyr-workspace/`, and `$ZEPHYR_BASE`'s parent. Two tests cover the
resolution, and they take the explicit pointer as a PARAMETER rather than
reading the env — the first version had both tests touching the same global and
it failed once, then passed 8 runs in a row, which is the worst kind of test.

`book/src/getting-started/integration-zephyr.md` documents the procedure and
says plainly that plain `west build` remains the primary flow: `nros build` is
the convenience that applies an image's overlays, never a required layer between
the user and west.

## What this does NOT do

It does not generate a west application, so a zephyr fixture row still names its
hand-written entry package. Whether those 14 rows can move off `entry =` now
depends on the harness fields 0892 lists below (`west_build_name`, `west_id`,
`west_zenoh_locator`), which are fixture concerns rather than image ones — that
remains open, and phase-383 W9.b records it.

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
