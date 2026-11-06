---
id: 2291
title: "`nros board new --out-of-tree <dir>` still demands a nano-ros checkout or `--workspace`, so a user without a checkout cannot scaffold a board"
status: resolved
type: bug
area: cli, boards, ux
related: [rfc-0064, rfc-0117, phase-537, phase-375, 2292, 2294]
found: 2026-11-06
resolved: 2026-11-07
resolved_in: "fix-issue-2291-2292 branch: the out-of-tree road resolves no checkout"
---

**Home: none yet.** Reported by the phase-537 W12 book session (rewriting the
Extending pages) with an in-tree CLI.

## Symptom

On the installed track, outside any checkout:

```
$ nros board new acme-board --platform freertos --out-of-tree ./boards
Error: could not auto-detect nano-ros workspace root from <cwd>; pass --workspace <path> explicitly
```

The command refuses unless the user is inside a nano-ros checkout or passes
`--workspace <checkout>`. That flag cannot help a user who has no checkout,
and the out-of-tree road writes nothing into one. The module doc says
`--out-of-tree` is "the whole difference between our board and a user's"
(RFC-0064 R5 D1). That is not true while the user's road needs our tree.

## Code site

`packages/cli/nros-cli-core/src/cmd/board_new.rs`, `run()`:

```rust
let root = match &args.workspace {
    Some(p) => p.clone(),
    None => crate::cmd::board::find_workspace_root()?,
};
let dir = match &args.out_of_tree {
    Some(base) => base.join(format!("nros-board-{}", args.name)),
    None => root.join("packages").join("boards")…,
};
```

`root` is resolved for every call. Only the in-tree arm uses it: it builds
`dir`, and `append_registry_row(&root, …)` runs only when `out_of_tree` is
`None`. `find_workspace_root()` (`cmd/board.rs`) walks up looking for
`packages/boards/` and fails otherwise. It does not use the SDK-root ladder
that `catalog_root()` uses, which `nros setup board` reads (phase-517 W2).

One related problem: `print_next_steps` prints `just check fast` as the
acceptance step on both roads. A user outside the checkout has no `just`
recipes. Issue 2294 covers that.

## Fix direction

- Resolve `root` only in the in-tree arm. With `--out-of-tree`, the
  command needs no checkout and should never search for one.
- If the in-tree arm still needs a root, read it through
  `board::catalog_root()`, the ladder every other `board` verb uses, not
  through a second walk.
- Add a test that runs `--out-of-tree` from a cwd with no checkout above it
  and with `NROS_WORKSPACE_ROOT` unset, and asserts the package is written.

## Resolution

`run()` now resolves a root only on the in-tree arm. With `--out-of-tree` it
neither reads `--workspace` nor walks for a checkout, so the command works from
any directory with `NROS_WORKSPACE_ROOT` unset. The registry row is appended
only when a root was resolved, which is the in-tree arm and nothing else.

The in-tree arm keeps `find_workspace_root()` rather than moving to
`board::catalog_root()`, against the fix direction's second bullet.
`catalog_root` is the READ ladder: its last rung is the running binary's own SDK
root (`share/nano-ros` beside a released `nros`), and the in-tree arm WRITES
`packages/boards/<pkg>` and edits `board-support.toml`. Following that ladder
would let an installed `nros board new` write into its own install prefix. The
doc comment on `find_workspace_root` already names `nros board new` as the verb
that needs a checkout, and that is still true of the in-tree arm alone.

Test: `out_of_tree_needs_no_checkout` in
`packages/cli/nros-cli/tests/board_new_scaffold.rs` runs the verb from a temp
directory that it asserts no checkout encloses, with no `--workspace` and
`NROS_WORKSPACE_ROOT` removed, and checks both files are written. It failed
before the change with the issue's error.

The `just check fast` next-step line is issue 2294's and is not changed here.
