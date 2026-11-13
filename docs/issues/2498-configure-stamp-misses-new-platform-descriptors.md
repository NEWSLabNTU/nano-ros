---
id: 2498
title: "The configure stamp records the files configure read, not the directory listings it searched, so a new platform or board descriptor added outside the project is not seen"
status: open
type: bug
severity: low
area: [cli, build]
found: 2026-11-13
related: [2478, 2485]
---

## What

Issues 2478 and 2485 made every file a configure reads outside the project
a stamp input (`configure_stamp::read_external`): board crate manifests,
board descriptors, the SDK index and the platform `nros-platform.toml` files.
The stamp records the digest of each file that was READ. It does not record
the listing of any directory that configure SEARCHED.

So adding a file that a search would now find leaves the stamp current:

- **Platform descriptors.** `PlatformsTree::load` lists every
  `<root>/*/nros-platform.toml` in each search root, and the first root to
  define a name wins. A new `<root>/<name>/nros-platform.toml` in an earlier
  root shadows the platform configure resolved against. A new platform that
  a board names would also resolve differently. Neither is seen until
  something else stales the stamp.
- **Board descriptors.** `BoardCatalog` walks `packages/boards/` and each
  `$NROS_EXTRA_BOARD_PATH` root for `nros-board.toml`. A new descriptor that
  claims a board name an image uses is the same case.

Not measured as a user-visible failure: found by reasoning about issue
2485's fix, whose archive records it as the remaining gap.

## Fix direction

Record each searched directory's relevant listing (the sorted names that
matched, or the absence of a match) as a stamp input beside the file
digests, and compare it in `currency`. The search sites live in two crates
(`nros-platform-config` and `nros-cli-core`), so the platform loader has to
report its listings the way `PlatformsTree::source_files` reports its files.
A regression test per search: add a shadowing descriptor after a configure
and expect the stamp to be stale.

`nros configure --force` re-runs configure in the meantime.
