---
id: 905
title: "A workspace copied out of the nano-ros checkout cannot resolve its leaf
  `.cargo/config.toml` include, and it is not established whether `nros sync`
  repairs it"
status: open
type: bug
area: cli, build
related: [issue-0457, issue-0463, issue-0285]
---

## Problem

A nano-ros workspace is supposed to live outside the nano-ros checkout
(RFC-0085 D1). Copy one there and the build dies during cargo's CONFIG load,
before any compilation:

```text
error: could not load Cargo configuration

Caused by:
  failed to load config include `../../../../../../nros-patch.toml` from
  `/tmp/nros-two-tree-mEDRK4/ws/src/zephyr_entry/.cargo/config.toml`
```

The leaf's generated `.cargo/config.toml` reads:

```toml
include = ["../../../../../../nros-patch.toml"]

[patch.crates-io]
nros-zephyr-build = { path = "../../../../../packages/tooling/nros-zephyr-build" }  # nros-managed
```

Both paths are relative and both climb out of the workspace into the nano-ros
checkout. In the tree they were generated for that resolves; one directory
elsewhere it does not.

Found by `scripts/dev/two-tree-check.sh` on its first run — the script exists
to build a workspace in `$TMPDIR` with the framework and Zephyr in two other
trees, which is the shape D1 describes and nothing previously exercised.

## The relative spelling is deliberate, which is why this is not a one-line fix

`ws.rs` states the split, and it is right:

* an **IN-TREE example leaf** has its `.cargo/config.toml` COMMITTED, so it uses
  a relative `include` — a host-absolute path would break every other checkout;
* an **OUT-OF-TREE consumer** does not commit one, so sync inlines the trio with
  ABSOLUTE paths and skips the include entirely, because the include has three
  fragile preconditions (cargo ≥ 1.93, a correct relative path, a present
  central file) and tripping any one fails with an unexplained
  `no matching package named 'nros'` (#272).

A workspace COPIED out of the checkout is a third case neither arm names: the
file on disk is the in-tree form, but its new location makes it an out-of-tree
consumer.

## What is NOT established

**Whether `nros sync` rewrites it.** CLAUDE.md says *"moved checkout → re-run
`nros sync`"*, and `two-tree-check.sh` does re-run it before building. So on the
face of it sync ran and left the file alone.

That is not a finding, because the first attempt to check it was invalid:
`nros sync` was invoked with stderr suppressed and never ran at all —

```text
Error: in-tree nros CLI is STALE — its sources changed since it was built
```

— so "sync leaves it untouched" was read off a run that did not happen. The
observation was corrected in the same session; what remains untested is the
same question asked properly.

**Which makes the first job here a measurement, not a patch:**

```sh
# 1. copy a workspace out, with a FRESH CLI (just setup-cli first)
scripts/dev/two-tree-check.sh --keep
# 2. read the leaf config in the copy
cat <copy>/ws/src/zephyr_entry/.cargo/config.toml
# 3. and check what sync decided, with stderr VISIBLE
cd <copy>/ws && NROS_REPO_DIR=<checkout> nros sync
```

If sync does rewrite it, the two-tree script is at fault for carrying a
generated file across (it excludes `build/`, `target*/`, `generated/` and
`.cargo/nros-managed-patch.toml`, but not `.cargo/config.toml`) and the fix is
one more `--exclude`. If sync does not, the fix is in sync: a leaf whose
recorded paths no longer resolve from its own location is an out-of-tree
consumer now, whatever it was when it was written.

## Why it matters

* It is the failure a real user hits FIRST. Their workspace has never been
  inside the checkout, so every path a copy inherits is one they never had —
  but the same arithmetic decides what sync writes for them.
* It fails during cargo's config load, four frames below anything that names
  `nros sync`, which is the issue-0463 class: *"a missing `include` target is a
  HARD cargo error during MANIFEST PARSE"*.
* The plan-level half of the two-tree case IS tested
  (`build_verb_pipeline.rs`: `the_zephyr_workspace_can_live_in_a_third_tree`,
  `a_plan_is_answerable_with_no_zephyr_on_the_machine`,
  `a_missing_framework_names_how_to_supply_it`). Every location decision the
  planner makes is covered; this is the one that happens after it.

## Sweep

```sh
grep -rn 'nros-patch.toml' packages/cli/nros-cli-core/src/cmd/ws.rs
grep -rln 'include = \[' --include=config.toml examples/workspaces/*/src/*/.cargo/
```

## Related

* RFC-0085 D15 — the two-tree case and what it did and did not establish.
* `scripts/dev/two-tree-check.sh` — the reproducer.
* #0457 / #0463 — the origin split for sync-managed rows, and why a missing
  include target is a hard parse error rather than a silent drop.
