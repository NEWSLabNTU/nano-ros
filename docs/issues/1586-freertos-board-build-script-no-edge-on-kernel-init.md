---
id: 1586
title: "nros-board-freertos's build script skips the kernel with no rerun edge, so initialising the submodule does not repair the build"
status: open
type: bug
area: [build, freertos]
severity: low
found: 2026-09-29
related: [issue-1527, issue-0196]
---

## What happens (measured)

In a fresh worktree `third-party/freertos/kernel` is an uninitialised
submodule. `packages/boards/nros-board-freertos/build.rs` probes
`$FREERTOS_DIR/tasks.c`, finds nothing, prints

    nros-board-freertos: FREERTOS_DIR is set but its kernel sources are absent (… tasks.c not found); skipping kernel / lwIP

and `return`s **before emitting any `cargo:rerun-if-changed`**. Its later
`rerun-if-changed={freertos_dir}` lines are never reached on that path.

That warning then tells you to run `git submodule update --init
third-party/freertos/kernel`. Doing so does NOT rerun the script:

- a script that emits no rerun line reruns only when its own package's files
  change;
- cargo replays the CACHED warning;
- the link fails with `undefined symbol: xQueueGenericCreate`,
  `nros_platform_panic`, `nros_platform_task_stack_unused_bytes`, … .

Measured on `just ci matrix build` (issue 1582's tier-2 run,
`rust-rtos-link-check`, `freertos_rs_talker`): it failed once before the
submodule init and again, identically, after it. It went on only after a
`touch` of `build.rs`.

So the build reproduces a state the tree no longer has. The cure is a wipe or
a `touch`, which is the missing-edge shape CLAUDE.md's `rm -rf` entry
describes.

## Fix direction

On the skip path, emit `cargo:rerun-if-changed=<the probed tasks.c>` (and the
kernel dir) before returning, so the input whose absence caused the skip is an
edge. Check sibling board build scripts that early-return on an absent SDK for
the same shape.
