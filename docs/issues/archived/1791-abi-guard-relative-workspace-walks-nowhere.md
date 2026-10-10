---
id: 1791
title: "`nros build --workspace .` measures the ABI guard against `NROS_REPO_DIR`
  instead of the checkout the workspace sits in — a relative anchor's walk-up
  never leaves `.`"
status: resolved
type: bug
area: [cli, build]
severity: medium
found: 2026-10-11
related: [0429, 1280, 1336]
---

## What was measured

In a linked worktree (`.claude/worktrees/<id>`) with that worktree's own
`nros` first on `PATH` and `nano_ros_ROOT` set to the worktree,
`scripts/build/workspace-fixtures-build.sh freertos-posix c` failed at
`nros build demo_bringup:freertos_posix --workspace . …` with

    Error: nros build aborted: ABI version mismatch between the `nros` CLI binary and the nano-ros runtime your build links.
      CLI emits codegen version:    10
      Runtime tree:                 /home/aeon/repos/nano-ros
      Runtime accepts:              2..=9

`/home/aeon/repos/nano-ros` is the MAIN checkout, a different tree at an older
commit. The shell carried `NROS_REPO_DIR=/home/aeon/repos/nano-ros` from the
user's profile. With `NROS_REPO_DIR` unset, the same command builds.

## Why

`cmd/build.rs::run` anchors the guard on `args.workspace` as given — here the
RELATIVE path `.` — and `abi_guard::runtime_root` tries arm 1, "a nano-ros tree
ABOVE the consumer", with `find_monorepo_root(start)`. That walk climbs with
`Path::parent`, and on a relative path `Path::new(".").parent()` is `Some("")`
and then `None`: it never reaches a real directory, so arm 1 finds nothing for
ANY relative anchor. The resolver then takes arm 2, `NROS_REPO_DIR`, which its
own doc comment says arm 1 must beat precisely because `NROS_REPO_DIR` is ambient
after `source activate.sh` ("a contributor with two checkouts open would
otherwise have every consumer in the second one measured against the first").
That is exactly what happened.

Without `NROS_REPO_DIR` the resolver falls to arm 3 (the tree above the `nros`
binary), which happens to be right when the binary is the worktree's own — so
the defect is invisible in CI and on a single-checkout host.

## Shape of a fix

Absolutise the anchor before the walk (`std::path::absolute`, which does not
require the path to exist and does not resolve symlinks) — in
`nros_launcher::checkout::find_monorepo_root`, so every caller of the one walk
gets it, rather than at the `nros build` call site alone. Sweep the other
callers of `find_monorepo_root` / `runtime_root` for relative arguments.

A test: `runtime_root(Path::new("."))` from a cwd inside a synthetic checkout,
with `NROS_REPO_DIR` pointing at a second synthetic checkout, must answer the
first.

## Resolution

Fixed on `fix/1791-abi-guard-relative-anchor`, in the ONE walk every caller
shares: `nros_launcher::checkout::find_monorepo_root` makes a relative `start`
absolute (`std::path::absolute`) before climbing. That covers the guard's
anchor and every other caller that can pass a workspace path as given
(`cmd/ws.rs` 824/3822/3984, `stale_guard.rs`, `pin.rs`, `rosidl-bindgen`),
rather than the `nros build` call site alone.

**Measured.** Before: the worktree build above refused with `Runtime tree:
/home/aeon/repos/nano-ros` whenever `NROS_REPO_DIR` named the main checkout,
and built with it unset. After: the test
`nros-launcher/tests/checkout_relative_anchor.rs` (its own binary, since it
changes the cwd) resolves `"."` and `"src/.."` from inside a synthetic checkout
to that checkout; with the walk reverted it FAILS (`left: None`).

Sweep: `git grep -n "find_monorepo_root(\|runtime_root(" -- packages`.

Not measured: the original worktree build re-run with `NROS_REPO_DIR` set,
on a CLI built from this branch.
