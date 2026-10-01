---
id: 1586
title: "nros-board-freertos's build script skips the kernel with no rerun edge, so initialising the submodule does not repair the build"
status: resolved
type: bug
area: [build, freertos]
severity: low
found: 2026-09-29
resolved: 2026-10-01
related: [issue-1527, issue-0196, issue-1580, issue-0490]
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

## Resolution

The absence is an input, so the skip path now declares it.
`nros_build_paths::watch_skip_cause(sdk_root)` emits
`rerun-if-changed=<sdk root>` when that root is a directory, and the caller
adds `rerun-if-changed=build.rs` (once a script emits any rerun line cargo
stops watching the package by default). It watches the ROOT, not the probed
file: an uninitialised submodule is an empty directory, populating it moves
the directory's mtime, and cargo scans a watched directory recursively.
Watching the absent `tasks.c` itself would be issue 0490's permanently-dirty
unit; an absent root declares nothing rather than an ancestor that could be a
whole filesystem.

Measured (`cargo build -p nros-board-freertos --target thumbv7m-none-eabi -v`,
fresh worktree, kernel submodule deinitialised):

- before: the skip run recorded **0** rerun lines; after `git submodule
  update --init third-party/freertos/kernel` the crate was `Fresh` and the
  cached "kernel sources are absent" warning replayed;
- after: the skip run records the kernel dir + `build.rs`; a no-op rebuild is
  `Fresh`; after the init, `Dirty nros-board-freertos: the file
  third-party/freertos/kernel has changed`, and all four archives
  (`libfreertos.a`, `liblwip.a`, `libnros_platform_freertos.a`,
  `libfreertos_glue.a`) are built.

Same shape, same fix, at its siblings: `nros-board-threadx`'s absent-port
skip (plus `THREADX_PORT` + `build.rs`), its port/target-mismatch skip
(`THREADX_PORT` + `build.rs` — the port NAME decided it), and both
`$NUTTX_DIR/include` skips in `nros_board_common::nuttx_platform_build`.
Left alone, on purpose: the "`FREERTOS_DIR` / `THREADX_DIR` not set" skips
(a path variable may not be fingerprinted as text, issue 0491) and the
TARGET-decided host-probe skips (a different TARGET is a different unit). Not
fixed here: `nuttx_ffi_build`'s skip on a missing `staging/libc.a` — that file
is a NuttX BUILD output, so watching for it is a different question from an
uninitialised source tree.
