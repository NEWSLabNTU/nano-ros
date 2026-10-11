---
id: 1810
title: "The provider scan walks gitignored trees inside the nano-ros root, so a staged SDK under `tmp/` makes every configure fail with `unknown RMW 'zenoh'`"
status: open
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
