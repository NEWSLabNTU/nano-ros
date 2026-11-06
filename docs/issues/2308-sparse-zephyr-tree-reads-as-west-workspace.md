---
id: 2308
title: "A sparse Zephyr source tree reads as a provisioned west workspace"
status: open
type: bug
area: [ci, zephyr, testing]
severity: medium
found: 2026-11-06
related: [1387, phase-543]
---

## Measured

CI's fast job provisions Zephyr's C++ minimal-libc headers with a sparse clone
(`gate.yml`, "Provision the sources the fast-line source gates read"):

    git clone --depth 1 --filter=blob:none --sparse --branch "$rev" \
      https://github.com/zephyrproject-rtos/zephyr.git zephyr-workspace/zephyr
    git -C zephyr-workspace/zephyr sparse-checkout set lib/cpp/minimal/include

`check-cxx-compat-shim-facilities` reads exactly that path, so the fast lane
needs the tree.

On 2026-11-06 the same clone was made in a local worktree, so the fast gates
could run as CI runs them (phase-543 W3). The next `just ci gate` failed in
`check::build`: `template-copy-out` built `ament-workspace-port/zephyr` and
reported "FAIL — the copy does not build". Before the clone, the same unit
recorded NOT VERIFIED ("Zephyr not provisioned — no west workspace resolves").
Removing the clone brought the earlier behaviour back.

The cause is the provisioning test in `scripts/check-template-copy-out.sh`:

    zws="$(bash scripts/lib/zephyr-workspace.sh --absolute resolve)"
    if [ -z "$zws" ] || [ ! -d "$zws/zephyr" ]; then  # "Zephyr not provisioned"

A sparse `zephyr-workspace/zephyr` holding one include directory passes it. It
is a source tree, not a west workspace: no `.west/`, no modules, nothing
`west build` can use.

## What it is

Two different capabilities share one path, and the test cannot tell them
apart:

- **zephyr-tree**: some Zephyr sources (here only `lib/cpp/minimal/include`),
  for gates that read headers.
- **zephyr**: a resolvable west workspace that can build an image.

phase-543 names them as two capabilities in `.config/gate-lanes.toml`. The
fast lane claims the first only, so under the nextest harness the build-side
gates stay out of the fast lane's scope. The old runner has no scope, so its
gates run whatever the test lets through.

## What would close it

"Is a west workspace provisioned" is answered in one place, and it asks
something a sparse source tree cannot satisfy (`west topdir` from the
candidate, or a `.west/config` naming the manifest). `template-copy-out`,
`zephyr-workspace-foreign-checkout` and every other reader of
`zephyr-workspace.sh resolve` take that answer. The sparse header tree moves
to a path of its own, or the readers that want headers alone ask for the
header path, never for "the workspace".
