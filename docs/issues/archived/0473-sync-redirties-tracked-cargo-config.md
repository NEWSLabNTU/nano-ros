---
id: 473
title: "`nros sync` writing `# nros-managed` rows into tracked `.cargo/config.toml` is BY DESIGN — this issue was filed on a wrong premise"
status: wontfix  # not a defect — analysed 2026-08-07, see Correction
type: tech-debt
area: build
related: [issue-0457, issue-0463, issue-0272]
---

## Correction (2026-08-07) — filed on a wrong premise

**This is not a defect, and the framing below is wrong.** Investigating "why are
these tracked" produced the opposite answer to the one the issue assumes.

The design is stated in `.gitignore:92-105`:

> A leaf whose config holds nothing else is pure sync output — `nros sync`
> recreates it from scratch — and 54 of them were tracked, churning on every
> sync… It cannot be scoped by path: leaves with hand-authored content
> (`[build] target`, a QEMU `runner`, link args) sit in the same directories,
> and sync only refreshes the patch block INSIDE them. **Those stay tracked.**

So a tracked config carrying `# nros-managed` rows is the INTENDED shape, not a
leak. The measurements:

| | |
| --- | --- |
| `.cargo/config.toml` on disk | 696 |
| tracked | 75 |
| tracked **containing `# nros-managed`** | **50** (133 rows) |
| tracked managed rows naming a `generated/` tree | **0** |
| worktree state at rest | **clean** |

Two facts kill the premise:

* It is **50 files, not 2.** Had this been a leak, two-thirds of every tracked
  config would be leaking — it is simply the documented category.
* Every managed row is a **repo-relative path to a committed in-tree crate**
  (`nros-platform`, `nros-rmw-zenoh`, the board crates, PACs). None is
  host-derived, and **zero** name a `generated/` tree. So they reproduce
  identically from a clone, which is exactly why they may be committed — and why
  `check-cargo-config-tracked` passes rather than being too narrow, as this issue
  claimed. Issue 0457 already moved the genuinely host-specific half to the
  gitignored `nros-managed-patch.toml`, and that split is holding.

The observed dirt was a **transient delta**, not steady state: a new dependency
(`nros-zephyr-build`) entered the graph, so sync added a row that was not yet
committed. Commit it once and it is stable — the worktree is clean at rest,
verified.

**What survives.** One sub-observation is independently real and does not depend
on any of the above: `zephyr_entry` and `zephyr_entry_robot1` name
`nros-zephyr-build = "*"` — a REGISTRY name for an in-tree crate. It resolves
correctly only while the patch is in the loaded config chain; outside it, it
resolves against PUBLIC crates.io, which is issue 0378's failure mode. Path-dep
would remove the exposure and the patch row together. Tracked there, not here.

**Lesson for the next filing.** The issue was written after seeing two dirty
files and reasoning from the 0457 rule, without first asking how many tracked
configs carry managed rows or whether the rows were host-derived. Either check
would have refuted it in a minute. Counting before theorising.

## Original filing (premise refuted — kept for the record)

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

## The governing principle

`**/.cargo/config.toml` is **gitignored** (`.gitignore:105`). Counted 2026-08-07:
**696 exist on disk, 75 are tracked** — tracking is the rare, deliberate
exception, not the norm. The same applies to generated package sources. Both
depend on the user's ROS environment, so a tracked copy asserts one host's
resolution for everyone.

A config is tracked ONLY for the hand-authored half a clone cannot regenerate —
`[build] target`, a QEMU `runner`, link rustflags, a user `libc` patch. The two
files here qualify on that basis (they carry the NuttX `libc` patch and
per-target CFLAGS), so untracking them outright is NOT the fix.

**Everything sync generates belongs in the gitignored sidecar, whatever it points
at.** That is what #457 established, and it is the rule the `# nros-managed` row
breaks — not because `nros-zephyr-build` is a `generated/` tree (it is not), but
because a sync-written row in a tracked file commits one host's view and
re-dirties every other worktree.

## Fix shape

1. **Route the row to `.cargo/nros-managed-patch.toml`**, like every other
   managed patch. This is the direct fix and needs no manifest changes.
2. **Tighten `check-cargo-config-tracked`**: an `# nros-managed` marker inside a
   TRACKED config is itself the failure. The marker is unambiguous and already
   written by sync, so the gate costs a grep. Today the gate asks only whether a
   tracked config patches an uncommitted `generated/` tree, which is why this
   passed — the rule was narrower than the principle it enforces (the audit
   pattern CLAUDE.md records: gates whose coverage is narrower than their rule).
3. **Optionally** path-dep `nros-zephyr-build` in the two zephyr entry leaves, so
   no patch row is generated at all. Independently worthwhile — a registry-named
   in-tree crate resolves against PUBLIC crates.io whenever the patch is absent
   from the loaded config chain (#378) — but it treats this instance rather than
   the class, so it does not substitute for (2).

## Notes

Observed repeatedly during phase-340 work (2026-08-06/07); discarded by hand each
time before rebasing. An earlier, possibly-related drift was noted in the same
sessions against six FreeRTOS `.cargo/config.toml` files (a dropped `[patch]`
line rather than an added one) — not re-confirmed since, and worth checking
whether it is the same mechanism in the other direction.
