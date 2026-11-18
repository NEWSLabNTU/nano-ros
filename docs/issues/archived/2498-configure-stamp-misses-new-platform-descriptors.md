---
id: 2498
title: "The configure stamp records the files configure read, not the directory listings it searched, so a new platform or board descriptor added outside the project is not seen"
status: resolved
resolved_in: "the change that archives this issue"
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

## Fix

The stamp now records what each search FOUND, beside the files it read.

- `configure_stamp::record_search(root, Search)` is the one spelling for a
  configure-path directory search, as `read_external` is for a file read.
  It stores the sorted list of directories the search finds under `root`
  (or `absent` for a root that cannot be listed) as a digest in
  `Stamp::external`. The key is `<root>/*/nros-platform.toml` or
  `<root>/**/nros-board.toml`, so it names both the root and the search.
- `currency` recomputes each listing with the same function the loader
  uses and reports a change as "the `<file>` files found under `<root>`
  changed (one added or removed)". Recorded file digests are compared
  first, so an edited or deleted file still names itself.
- The listings come from one place per search:
  `platform_config::platform_descriptor_dirs`, which `PlatformsTree::load`
  and the cargo road's `platform_descriptor_files` now both use, and
  `BoardCatalog::collect_board_dirs`.
- Recorded at `configure_stamp::load_platforms_tree` (every root, missing
  ones included) and `BoardCatalog::load_root`.

The sweep found the same gap INSIDE the project. A read there is left to the
project digest, and the digest walked only `INPUT_NAMES`, which held neither
`nros-board.toml` nor `nros-platform.toml`. So adding a descriptor to the
user's own workspace package, or editing one there, left the stamp current.
Both names are digest inputs now.

`STAMP_FORMAT` is 6. A format-5 stamp recorded no listing, so it is stale
once.

Tests, each red with the fix disabled:

- `a_platform_descriptor_added_to_a_searched_root_stales_the_stamp`: a
  shadowing descriptor in an earlier root that did not exist at configure;
- `a_board_descriptor_added_to_a_searched_root_stales_the_stamp`: a
  descriptor two levels under an extra board root;
- `a_descriptor_inside_the_project_is_a_digest_input`: both names inside
  the project.

## Not changed

The cargo road (`platform_config`'s build-script rung) still watches each
descriptor FILE with `rerun-if-changed`, so a descriptor added to a root
after a build does not rerun that build script. Cargo has no
directory-listing watch: `rerun-if-changed=<dir>` takes the newest mtime
of everything below it, and `packages/platform` holds whole crates, which
is the reason the per-file watch was chosen (issue 1401). A configure
reached through `nros build` re-resolves first, so this only affects a
bare `cargo build` of a leaf.

Sweep: `git grep -n 'load_search_path\|collect_board_dirs\|platform_descriptor_dirs\|record_search' -- packages/cli packages/tooling`
