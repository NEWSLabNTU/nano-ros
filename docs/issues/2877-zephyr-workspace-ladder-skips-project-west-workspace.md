---
id: 2877
title: "The Zephyr workspace ladder never looks at the project's own west workspace, so a project with its own Zephyr patches builds against the store's tree unless every call names it"
status: open
type: bug
severity: medium
area: [cli, zephyr, build]
related: [phase-478, 1767, 2697, 2863, 2766]
found: 2026-11-23
---

**Home:** [phase-478](../roadmap/phase-478-island-developer-experience-findings.md)
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
