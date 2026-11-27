---
id: 2996
title: "The nano-ros tree's provider scan finds test-fixture packages, so `nros knob list` lists their knobs"
status: resolved
type: bug
severity: medium
area: [cli, knobs, providers, tests]
related: [phase-569, phase-527, phase-581]
found: 2026-11-27
resolved: 2026-11-27
resolved_in: "fix-knob-scan-fixtures"
---

## What happened

`nros knob list`, run anywhere, listed two knobs nano-ros does not ship:

    acme.pool_slots                    acme_rmw                         nano-ros tree (root 0)
    fixture_r5f_freertos.uart_rx_slots nros_board_fixture_r5f_freertos  nano-ros tree (root 0)

Both are test fixtures under `packages/testing/nros-tests/fixtures/`.

## Cause

Every workspace's provider search path starts with the nano-ros tree itself
(root 0), and that walk descends into test fixture trees. So a fixture package
is a provider every user workspace resolves (`rmw = "acme"` would find the
out-of-tree-rmw fixture), and the knob catalog lists its rows.

The walk already had the mechanism: an ignore marker (`COLCON_IGNORE`) prunes
a directory's descendants from any walk that starts above it, and never a root
the caller names. phase-527 W4 put one on a single fixture directory
(`nros-cli-core/tests/fixtures/extension_packages`), for the same reason, and
the other fixture trees kept no marker.

## Fix

An ignore marker at the root of both fixture trees,
`packages/testing/nros-tests/fixtures/` and `packages/cli/nros-cli-core/tests/fixtures/`.
A test that uses a fixture as its workspace names it as the root, so it is
still scanned.

A workspace INSIDE the nano-ros tree used to be dropped as a search root,
because the tree walk covered it. Under a marker the walk no longer does, so
`provider_scan::search_path_over_scoped` keeps such a workspace as its own
root when an ignore marker sits on it or on a directory between it and the
tree. Without that, the out-of-tree-rmw build tests (which copy the fixture
under the checkout's `tmp/`, below a marker) lost their `acme` provider:
they had been passing on the tracked fixture the tree walk found, not on the
copy they built.

## Test

`packages/cli/nros-cli-core/tests/tree_scan_excludes_fixtures.rs`:
- the scan of the nano-ros tree finds no provider below `packages/testing`, a
  `tests/` or a `fixtures/` directory. This is the class: a fixture tree added
  anywhere else without a marker fails it, naming the package;
- the catalog of an empty workspace (what `nros knob list` prints) has no
  knob from a package that is not built in;
- the negative control: the out-of-tree-rmw fixture named as the root is still
  scanned and its provider found;
- `provider_scan::a_nested_workspace_under_an_ignore_marker_is_its_own_root`:
  a workspace under a marked directory in the tree is its own root, an unmarked
  sibling stays covered.

`just check cli-tests` passes, including the three out-of-tree-rmw build tests.

With the two markers removed, all three fail, listing the two fixture
packages and their two knobs.

`nros knob list` measured 0.02 s from an empty directory on 139 (no slowdown);
the 2 min reported was during a /home I/O stall, so nothing is filed for it.
