---
id: 473
title: "`nros sync` writes an `# nros-managed` patch row into TWO tracked `.cargo/config.toml` files, re-dirtying the worktree on every build"
status: open
type: tech-debt
area: build
related: [issue-0457, issue-0463, issue-0272]
---

## Symptom

A fixture build that syncs the Rust workspaces leaves the worktree dirty:

```console
$ just nuttx build-fixtures     # or any build that syncs these workspaces
$ git status --short
 M examples/workspaces/realtime-rust/.cargo/config.toml
 M examples/workspaces/rust/.cargo/config.toml
```

The diff is one added line in each, inside a **tracked** file:

```diff
 [patch.crates-io]
 libc = { path = "../../../third-party/nuttx/libc" }
+nros-zephyr-build = { path = "../../../packages/tooling/nros-zephyr-build" }  # nros-managed
```

Discard it and the next sync writes it again.

## Why it is wrong

Issue 0457 moved sync's managed `[patch.crates-io]` block out of the tracked
`config.toml` and into the gitignored sidecar `.cargo/nros-managed-patch.toml`,
precisely because a managed row inside a tracked file re-dirties the worktree on
every sync. These two files still receive one.

`check-cargo-config-tracked` **passes** on it, so this is not a red gate — the
gate's rule is that a tracked config must not patch an *uncommitted `generated/`
tree*, and `nros-zephyr-build` is a committed in-tree package. The row is
therefore legal by the letter of the gate while being exactly the shape 0457
removed.

## Root cause

Two zephyr entry leaves name the crate by REGISTRY name:

```
examples/workspaces/rust/src/zephyr_entry/Cargo.toml:115:        nros-zephyr-build = "*"
examples/workspaces/rust/src/zephyr_entry_robot1/Cargo.toml:117: nros-zephyr-build = "*"
```

A registry-named dependency is what obliges sync to emit a `[patch.crates-io]`
row at all. A path dependency would need no patch, no managed row, and no
sidecar entry.

That makes this the same class CLAUDE.md already states for message crates —
*never registry-name an in-tree crate in a leaf manifest* — applied to a tooling
crate instead. `nros-zephyr-build = "*"` also resolves against the PUBLIC
crates.io whenever the patch is not in the loaded config chain, which is issue
0378's failure mode.

## Fix shape

Preferred: make the two leaves path-depend on `nros-zephyr-build`, so no patch
row exists to misplace. That removes the symptom and the 0378 exposure together.

Failing that: route the row to the `nros-managed-patch.toml` sidecar like every
other managed patch, and tighten `check-cargo-config-tracked` so an
`# nros-managed` marker inside a tracked config is itself the failure — the
marker is unambiguous and would have caught this.

## Notes

Observed repeatedly during phase-340 work (2026-08-06/07); discarded by hand each
time before rebasing. An earlier, possibly-related drift was noted in the same
sessions against six FreeRTOS `.cargo/config.toml` files (a dropped `[patch]`
line rather than an added one) — not re-confirmed since, and worth checking
whether it is the same mechanism in the other direction.
