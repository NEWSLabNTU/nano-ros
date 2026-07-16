---
id: 208
title: "stale setup.bash survives beside activate.sh — divergent env SSoT, still advertised by `just setup` and 3 book pages"
status: open
type: tech-debt
area: ux
related: []
---

## Problem (audit 2026-07-16, F3/H1)

Phase-218.C made `activate.sh`/`activate.fish` the activation SSoT, but:

- Root `setup.bash` still exists and exports `NROS_ROOT` while activate.sh
  exports `NROS_REPO_DIR` + `nano_ros_ROOT` — two divergent env contracts.
- `justfile:2235,2270` (`just setup` menu + post-setup hint) tell users to
  `source ./setup.bash`.
- `book/src/getting-started/zephyr.md:72`,
  `book/src/internals/contributing.md:14`,
  `book/src/reference/build-commands.md:95` still reference it.

## Fix sketch

Make setup.bash a one-line `source ./activate.sh` shim (or delete it),
repoint the justfile hints and the 3 book pages at activate.sh.
