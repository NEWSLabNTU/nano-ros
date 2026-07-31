---
id: 363
title: "`nros sync` fails in the metadata-mode harness — and a failed run leaves the tracked `.cargo/config.toml` PARTIALLY REWRITTEN, silently dropping a patch entry"
status: open
type: bug
severity: medium
area: build, cli
related: [issue-0320, issue-0359, rfc-0048]
---

## Finding (2026-07-31, while fixing the `check-leaf-lockfiles` red)

`nros sync` is the documented remedy for a stale central `nros-patch.toml`
(RFC-0048 W9; CLAUDE.md: *"Never hand-edit; moved checkout → re-run `nros
sync`"*). It does not currently work, and failing is not the worst part.

## Two faults, in order of discovery

### 1. A stale `nros-launch-resolve` skews against the CLI

```
Error: ws sync: nros-launch-resolve failed for `demo_bringup`
error: unexpected argument '--bringup-root' found
```

The in-tree CLI passes an argument the installed helper does not accept. They are
built by **different** recipes — `just setup-cli` and `just setup-launch-resolve`
— so rebuilding one silently skews it against the other. `just setup-cli` had
just been run (its own staleness check demanded it), which is exactly when this
bites.

`just setup-launch-resolve` clears it. Nothing said to run it: the CLI's
staleness probe checks only itself, and `just doctor` did not flag the pair.
Two binaries that must agree on an argument list, with no gate over the pair —
issue 0359's shape (a committed artifact nobody re-derives), one layer over.

### 2. With that fixed, sync fails deeper

```
Error: refresh source metadata for `action_client_pkg`
Caused by: metadata-mode harness failed (exit 101) for component 'fibonacci_client'
Caused by: No such file or directory (os error 2)
  at nros-cli-core/src/orchestration/metadata_build.rs:327
```

Not diagnosed here. The `No such file or directory` alongside phase-321's package
moves (`packages/core/nros` → `packages/api/nros`, and others) suggests a path
the harness still resolves against the old layout, but that is a guess and is
labelled as one.

## The part that is worse than the failure

**The failed run had already rewritten a TRACKED file**, and the rewrite was
lossy:

```diff
 nros-rmw = { path = "../../../packages/core/nros-rmw" }  # nros-managed
-nros-zephyr-build = { path = "../../../packages/tooling/nros-zephyr-build" }  # nros-managed
 std_msgs = { path = "generated/std_msgs" }  # nros-managed
```

`examples/workspaces/rust/.cargo/config.toml` lost a `# nros-managed` patch entry
and kept the rest. The command exited non-zero, so a careful operator knows
something failed — but the damage is in a file they did not edit, in a repo they
may then commit from. `git checkout` restored it here only because the file is
tracked and the diff was noticed.

That is the real defect: **a generator that writes before it can finish.** Sync
should stage its output and move it into place only on success, so a failure
leaves the tree exactly as it found it. A partial `[patch.crates-io]` table is
worse than a stale one — a stale table resolves the wrong path loudly, a table
missing an entry resolves the dependency from crates.io *silently*, which is
issue 0359's own thesis (a lock/patch artifact that looks authoritative and is
not consulted).

## Why it surfaced now

`check-leaf-lockfiles` was red because the central `nros-patch.toml` still
pointed at `packages/core/nros` after phase-321 moved it. That file is generated
and gitignored, so **every checkout predating the move has a stale one**, and the
documented fix is the command that is broken.

Note the generated file's own header says *"re-run `nros sync` after moving the
checkout"* — it anticipates the checkout moving, not a PACKAGE moving inside it.
There is also no root-level regeneration verb: `nros sync` at the repo root
exits with *"expected colcon-style workspace or single-pkg dir"*, so the only way
to regenerate the central table is to sync some workspace and rely on the side
effect.

## Ways to fix

1. **Make sync atomic** (the important one). Write to a temp file per target and
   rename on success. Independent of why sync currently fails.
2. **Gate the CLI/helper pair.** `just setup-cli` should either rebuild
   `nros-launch-resolve` or fail when it is older; `just doctor` should report
   the skew. One of the two, not a comment.
3. **Diagnose fault 2.** Needed before `nros sync` is usable at all.
4. **A root-level regeneration verb** for `nros-patch.toml`, so recovering from a
   package move does not require picking a workspace and hoping.

`check-leaf-lockfiles` no longer depends on any of this — it now classifies a
missing/stale patch table as *not checkable* rather than as breakage — so this is
not blocking that gate.
