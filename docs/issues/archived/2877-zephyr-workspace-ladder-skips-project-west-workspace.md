---
id: 2877
title: "The Zephyr workspace ladder never looks at the project's own west workspace, so a project with its own Zephyr patches builds against the store's tree unless every call names it"
status: resolved
type: bug
severity: medium
area: [cli, zephyr, build]
related: [phase-478, 1767, 2697, 2863, 2766]
found: 2026-11-23
resolved: 2026-11-23
resolved_in: "fix(cli, issue 2877): the Zephyr ladder takes the project's own west workspace"
---

**Home:** [phase-478](../../roadmap/phase-478-island-developer-experience-findings.md)
I37. Consumer: the safety island (autoware-safety-island fork), branch
`nano-ros-next`, nano-ros pin `8d4e70a382`.

## What happens

`nros build` and `nros run` pick the Zephyr tree by one ladder
(`cmd/build.rs` `zephyr_base_with`, RFC-0095 D4): `--zephyr-workspace`, then
`$ZEPHYR_BASE`, then `$NROS_ZEPHYR_WORKSPACE`, then the store's
`$NROS_HOME/workspaces/zephyr/<line>`, then two trees relative to the
nano-ros checkout (`zephyr-workspace/`, `../nano-ros-workspace`). No rung is
the west workspace the PROJECT itself lives in: a downstream project that is
its own west manifest authority (`west.yml` beside its `system.toml`s,
`.west/` at its root, nano-ros as an `import: false` leaf) is skipped, and the
store's workspace wins with no line saying so.

That store tree is provisioned for nano-ros's own pins and patch set, not the
project's. So a project that carries a Zephyr patch nano-ros does not
(the island's `patches/zephyr/0003-tracing-allow-an-out-of-tree-backend.patch`,
see issue 2878), or pins its own HAL modules (`hal_nxp`, `cmsis` for the
S32Z270), builds against a tree without them. Measured on the island's FVP
lane: a Kconfig abort on `NET_SOCKETS_POLL_MAX`; in CI the store has no Zephyr
checkout at all.

## Island workaround

Every `nros build` and `nros run` on the FVP lane names the checkout:

- `.github/scripts/run-zephyr-fvp-ci.sh:202-213` (`nros_fvp`): `"${NROS_CLI}"
  "${verb}" --zephyr-workspace "${ROOT_DIR}" "$@"`, with the comment "Without
  it nros falls back to the store's Zephyr workspace, which has neither this
  repo's Zephyr patches nor, in CI, a checkout at all."
- Before issue 2863 gave `nros run` the flag, the lane exported
  `NROS_ZEPHYR_WORKSPACE` instead.

A developer typing `nros build controller_bringup:fvp` in `actuation_module/`
gets the store tree.

## Wanted

The ladder finds the project's own west workspace: walking up from the
nano-ros workspace being built, the first directory with `.west/` whose
`zephyr/` is a Zephyr tree (west's own `topdir` rule) is a rung, above the
store's. When it wins over a store tree that also exists, the build says
which tree it took (one line), as it does for the SDK.

Open for the owner: whether the project rung goes above or below
`$NROS_ZEPHYR_WORKSPACE` (the env var is ambient state; the project rung is a
fact of the tree being built).

## Acceptance

- A test of `zephyr_base_with` (or its successor) with a workspace nested in a
  west workspace returns that workspace's `zephyr/`, ahead of a present store
  tree; with no `.west/` above, the store still wins.
- `nros build` prints the tree it took when the project rung decided it.
- The island drops `--zephyr-workspace` from `run-zephyr-fvp-ci.sh` and its FVP
  lane stays green.

## Resolved, 2026-11-23

A new rung, `project_west_zephyr` (`cmd/build.rs`), between
`$NROS_ZEPHYR_WORKSPACE` and the store. The ladder is now:
`--zephyr-workspace`, `$ZEPHYR_BASE`, `$NROS_ZEPHYR_WORKSPACE`, the project's
west workspace, the store's `$NROS_HOME/workspaces/zephyr/<line>`, then the
checkout-relative trees. `nros run` builds through the same ladder.

- **The open question, answered conservatively: below the env var.** The new
  rung moves no existing one. A build that set `$NROS_ZEPHYR_WORKSPACE` keeps
  the tree it had; the only builds that change are those that fell to the
  store while standing in a west workspace of their own, which is the bug.
  Putting it above the env var would also have changed builds that chose a
  tree on purpose and never had the problem.
- **What counts as the project's west workspace** is west's own `topdir`
  rule plus a Zephyr check: the NEAREST directory at or above the workspace
  root that holds `.west/` (a farther one is never consulted, as west never
  does), whose `.west/config` `[manifest] path`/`file` (default `west.yml`)
  names a manifest that either lists a project `name: zephyr` (its `path:`,
  else `zephyr`) or is itself a Zephyr tree. The directory must hold
  `Kconfig.zephyr`. A `.west/` whose manifest names no `zephyr`, one not yet
  `west update`d, or an unreadable config or manifest is skipped, and the
  ladder goes on to the store as before.
- **The line:** when the project rung decides, `nros build` prints once per
  process `nros build: Zephyr tree <zephyr> (the west workspace <topdir> this
  project is in, not the store's <store>/zephyr; --zephyr-workspace or
  ZEPHYR_BASE names another)`, the store clause only when a store tree exists.
- **The shell ladder** (`scripts/lib/zephyr-workspace.sh`, used by `just` and
  the fixture builds) is unchanged: it serves nano-ros's own tree, whose
  workspace is never a project's.

Tests (`cmd/build.rs`, `zephyr_base_tests`):

- `the_project_s_own_west_workspace_wins_over_the_store`: a workspace
  nested in a west workspace whose manifest names Zephyr returns its `zephyr/`
  ahead of a present store tree, from the workspace root and from below it;
  the line names the tree, the top directory and the store tree passed over.
- `a_named_tree_still_wins_over_the_project_west_workspace`: the flag,
  `$ZEPHYR_BASE` and `$NROS_ZEPHYR_WORKSPACE` each still win.
- `without_a_zephyr_west_workspace_above_the_store_still_wins` (the
  controls): no `.west/` above; a manifest with no `zephyr` project (even
  beside a `zephyr/` directory); `zephyr` named but not checked out; a nearer
  `.west/` without Zephyr in front of a farther one with it.
- `the_project_rung_follows_the_manifest_s_zephyr_path`: a `path:` of the
  Zephyr project, and a manifest repository that is Zephyr itself.
- `a_west_config_value_is_read_from_its_section`.

The island's half (dropping `--zephyr-workspace` from
`run-zephyr-fvp-ci.sh`, FVP lane green) is phase-478 I37's, on the island's
`nano-ros-next` once its pin passes this fix.
