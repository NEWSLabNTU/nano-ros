---
id: 2371
title: "The four zephyr-west-app book probes are red on main: the probe image has no ROS 2, and the Rust track's west workspace is never patched"
status: resolved
type: bug
severity: medium
area: [ci, book, zephyr, probe]
related: [2278, 2344, 2185, 2358, phase-548]
found: 2026-11-09
---
**Home: [phase-548](../../roadmap/phase-548-three-languages-every-board.md)** (the west-application chapter is how a Zephyr user reaches every board in that phase).

## What fails

`probe-platforms.yml`, re-dispatched on main 2e391623c6 (run 42597019657,
2026-11-09 09:55 UTC), 9 of 24 jobs red. Five are `[UNMET PRECONDITION]`
tracks that declare their `awaits` (zephyr, zephyr-rust, renode-nucleo,
pinned, pinned-newer) and are expected. The four `zephyr-west-app*` tracks
(`scripts/probe/probe-tracks.json`, all `weekly: true`, chapter
`guide/platforms/zephyr-west-app.md`) declare no `awaits` and fail for real.
The scheduled run 42592266828 (09:17 UTC, main 29e3292198) failed the same
four the same way.

### C, C++ and flash tracks: no ROS 2 in the probe container

`book probe — zephyr-west-app`, `-cpp`, `-flash` stop at the west configure:

```text
activate.sh: no ROS 2 sourced and /opt/ros/humble/setup.bash not found.
CMake Error at /root/.nros/sdk/nros/0.0.0-probe/share/nano-ros/cmake/NanoRosBundledInterfaces.cmake:109 (message):
  nano-ros: the message package `builtin_interfaces` is on neither
  NROS_INTERFACE_SEARCH_PATH nor AMENT_PREFIX_PATH, and nano-ros does not
  generate it from its bundled copy in place of your ROS install (issue
  2278).
```

This is issue 2278's refusal working as designed (the user ruling: messages
must come from ROS; the bundled copy is only a workaround for internal deps).
The defect is the probe environment for these tracks: it has no
`/opt/ros/humble`, so the chapter can never reach a build. The same refusal
on the tier-2 runner is issue 2344; this is the book-probe side.

### Rust track: the west workspace is never patched

`book probe — zephyr-west-app-rust` stops at
`nros_check_zephyr_rust_cargo_patch` (`share/nano-ros/zephyr/CMakeLists.txt:127`):

```text
  /root/my_zephyr_ws/modules/lang/rust/CMakeLists.txt has no `# nano-ros:
  board facts` line ... (issue 2185).
  Apply nano-ros's Zephyr patch set to the workspace once, then configure
  again:
      nros setup board native_sim/native/64 --zephyr-workspace /root/my_zephyr_ws
```

So either the chapter's Rust path does not tell the reader to run that
`nros setup board ... --zephyr-workspace` step, or the probe skips it. Issue
2358's fix (merged before this run, 8246ad41aa) made the printed west entry
FETCH zephyr-lang-rust; it does not patch it.

## What it is NOT

- Not issue 2315 (router collision): those 7 tracks went green on this rerun.
- Not issue 2369 (GitHub release 504): every step ran past the installs.
- Not a nano-ros build regression: both refusals are deliberate guards
  reporting a missing prerequisite.

## What would close it

1. The west-app tracks run in an image with ROS 2 (or source it the way the
   chapter tells the reader to), OR the chapter states ROS 2 as a prerequisite
   and the track declares that in `awaits` until it can provide it.
2. The chapter's Rust path includes the workspace-patch step (or `nros setup
   board` applies it when it writes the west entry), and the Rust track replays it.
3. A weekly `probe-platforms` run with all four `zephyr-west-app*` jobs green.

## Resolution (2026-11-09)

The chapter, not the probe image, was wrong on both counts: the image is
`ros:humble`, but its entrypoint is disabled for the replay, as a reader's
fresh terminal is unsourced too.

- Both `west build` blocks (native_sim and the board) now source
  `/opt/ros/humble/setup.bash` first; before, only the Rust path's
  `nros configure` step did.
- The Rust step now runs `nros setup board zephyr --zephyr-workspace "$PWD"`,
  which the prose already said it did.

`probe-platforms` dispatched on the fix branch (run 42616239077): all four
`zephyr-west-app*` jobs green.
