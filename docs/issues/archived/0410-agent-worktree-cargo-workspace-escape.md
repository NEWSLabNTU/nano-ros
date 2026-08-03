---
id: 410
title: A git worktree inside the checkout is claimed by the OUTER workspace, so
  excluded leaf crates fail to resolve there
status: resolved
type: bug
area: build
related: [phase-337, 0363]
resolved_in: root Cargo.toml `exclude = [".claude", …]`
---

## Problem

Agent worktrees live at `.claude/worktrees/agent-*`, i.e. INSIDE the main
checkout. Cargo resolves a package's workspace by walking UP the directory
tree, so from a package in a worktree it escapes the worktree and reaches the
main checkout's root `Cargo.toml`. Every path in that manifest's `exclude` list
is relative to it, so none of them match a `.claude/worktrees/...` path, and
the outer workspace claims a package belonging to the inner one:

```
$ cargo locate-project --workspace     # in .../worktrees/agent-X/packages/boards/mps2-an385-pac
error: current package believes it's in a workspace when it's not:
current:   /home/aeon/repos/nano-ros/.claude/worktrees/agent-X/packages/boards/mps2-an385-pac/Cargo.toml
workspace: /home/aeon/repos/nano-ros/Cargo.toml
```

Only crates that are `exclude`d by the workspace root AND carry no `[workspace]`
table of their own are affected — board crates, PACs, verification crates. A
standalone copy-out example with its own `[workspace]` is immune, which is why
this hid for so long: the leaves people usually poke at in a worktree are fine.

## Evidence

Found 2026-08-04 by two phase-337 agents independently, in worktrees whose own
diffs were clean:

- `check-leaf-lockfiles` failed for ~20 untouched crates (`mps2-an385-pac`,
  `nros-board-stm32f4`, `fvp-aemv8r-smp`, `threadx-*`, `nros-verification`, …).
- `cargo fmt --all` failed.
- The `workspace-rust-qemu-freertos` and `large-msg-baremetal` fixture builds
  failed, the former as `error inheriting serde from workspace root manifest`
  via `nros-pkg-index` — a symptom that names neither cargo's walk-up nor the
  worktree.
- Control: identical manifests at the same commit, `cargo metadata` on
  `examples/workspaces/rust` SUCCEEDS in `/home/aeon/repos/nano-ros` and FAILS
  in the worktree.

The failure mode is the expensive kind: every message points at a package the
agent never touched, so the natural reading is "my change broke 20 crates".
Both agents spent real effort before concluding it was environmental, and one
could not run `just lock-update` at all and hand-edited a lockfile line instead.

## Fix

`exclude = [".claude", …]` in the root `Cargo.toml`. A worktree's leaves then
resolve against the worktree's own root, which is what the worktree is for.

Verified by reproducing the error, applying the exclude, and re-running
`cargo locate-project --workspace` in the same directory — it now reports the
package's own manifest inside the worktree.

## Note for future parallel sessions

The exclude fixes resolution, but a worktree is still a separate checkout: it
starts with no submodules, no built CLI, and no built fixtures. Every agent in
this batch had to run `git submodule update --init` for the submodules its lane
needed plus `just setup-cli && just setup-launch-resolve` before it could build
anything. That is setup cost, not breakage — but an agent that does not know it
reads the first failure as a repo defect.

One related gap worth knowing: editing a crate that `nros-cli-core` reaches
through a path dependency moves the CLI's source stamp WITHOUT cargo re-running
its build script, so `just setup-cli` can report success while leaving the
stamp stale. Touching `packages/cli/nros-cli-core/build.rs` forces it.
