---
id: 1810
title: "The provider scan walks gitignored trees inside the nano-ros root, so a staged SDK under `tmp/` makes every configure fail with `unknown RMW 'zenoh'`"
status: resolved
type: bug
area: [cli, build]
severity: medium
found: 2026-10-11
related: [phase-485, 1565]
---

## Measured

While testing phase-485 W10, an SDK root was staged at
`tmp/p485/stage/share/nano-ros` with `scripts/stage-sdk-root.sh`. That
directory is inside the checkout and gitignored. Every C/C++ configure in
the checkout then failed:

```
CMake Error at cmake/NanoRosFeatureSet.cmake:154 (message):
  nros_feature_set: unknown RMW 'zenoh' (announced by a provider:
  cyclonedds;uorb;xrce;zenoh; or 'none')
```

The message contradicts itself: `zenoh` is in the announced list. The cause
is behind `ERROR_QUIET` in `nros_rmw_is_known`. Run directly, `nros ws
rmw-dispatch zenoh --lines` says the name is announced TWICE:

```
  packages/rmw/zenoh/nros-rmw-zenoh
  tmp/p485/stage/share/nano-ros/packages/rmw/zenoh/nros-rmw-zenoh
```

`provider_search_path` scans the nano-ros root recursively and descends into
`tmp/`. In `just ci gate` this took out `probe-workspace-caps` and
`probe-shared-types`. Deleting the staged tree cleared it.

## Two defects

1. **The scan's reach is wider than its subject** (issue 1565's class: a walk
   from the checkout root sees other people's and other builds' trees). Only
   tracked provider descriptors, plus the configured search roots, should
   count in a checkout.
2. **The refusal hides its own cause.** `nros_rmw_is_known` swallows the CLI's
   stderr, so a DUPLICATE reads as UNKNOWN. That is the opposite of what
   happened, and it sends the reader to check a list that does contain the
   name.

## Fix

Scope the scan in a checkout to tracked descriptors (`git ls-files
'*nros-rmw.toml'`) or to `scripts/lib/repo_walk.py`'s rule, and have
`nros_rmw_is_known` pass the CLI's diagnostic through on a non-zero exit.

## Fixed

The root cause was narrower than defect 1 says. The duplicate was a staged
SDK root, and an installed SDK is exactly the "vendored nano-ros" that issue
0621's `.nros-ignore` exists for. The repo root carries that marker, so a walk
from above skips a nested checkout. `scripts/stage-sdk-root.sh` did not ship
it, so a staged or installed SDK was the one copy of nano-ros a walk from
above did not skip.

1. `stage-sdk-root.sh` ships `.nros-ignore` and verifies it is there. An SDK
   used as the nano-ros root is unaffected, because a marker never prunes the
   root it sits in.
2. `nros_rmw_is_known` now returns the CLI's diagnostic in
   `<out_var>_WHY`, and both callers (`nros_feature_set`, `nros-cpp`) print
   it. They also say "does not resolve", not "unknown".

Scoping the walk to tracked files was not done. A user workspace that keeps
its imported sources gitignored (a common `vcs import` layout) would lose
them, and the walk would need a git subprocess per root.

## Evidence

- With the marker, `nros ws rmw-dispatch zenoh --lines` exits 0 while an SDK
  staged under `tmp/` is present.
- With the marker removed (negative control), it exits 1, naming both
  copies.
- A configure of `examples/native/c/talker` with the marker removed now
  fails with:

  ```
  nros_feature_set: RMW 'zenoh' does not resolve (...). The provider scan says:
  Error: rmw `zenoh` is claimed by 2 packages in the SAME search root ...
  ```

  With the marker present, the same configure passes.
- `nros ws providers` with the staged SDK as its own root still lists its rmw
  providers.
