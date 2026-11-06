---
id: 2291
title: "`nros board new --out-of-tree <dir>` still demands a nano-ros checkout or `--workspace`, so a user without a checkout cannot scaffold a board"
status: open
type: bug
area: cli, boards, ux
related: [rfc-0064, rfc-0117, phase-537, phase-375, 2292, 2294]
found: 2026-11-06
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
