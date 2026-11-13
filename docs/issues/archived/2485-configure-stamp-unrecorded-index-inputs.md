---
id: 2485
title: "The configure stamp does not record the SDK index or the platform `nros-platform.toml` files, which configure reads outside the project"
status: resolved
type: bug
severity: medium
area: [cli, build]
found: 2026-11-13
related: [2478, 2480, 2498]
resolved: 2026-11-13
resolved_in: "`SdkIndex::load` reads through `configure_stamp::read_external`; the platform tree reports the descriptors it read and `configure_stamp::load_platforms_tree` records them"
---

> **Resolved (2026-11-13).** The SDK index and every `nros-platform.toml`
> a configure reads are stamp inputs now. See "Fix".

## What

Issue 2478 made every configure-path read of a file outside the project a
stamp input, through `configure_stamp::read_external`. The sweep that fixed
it covered the board crate manifests (`Cargo.toml` feature tables) and the
board descriptors (`nros-board.toml`). It found two more inputs outside the
project that configure reads and the stamp still does not record:

- **`nros-sdk-index.toml`** at the nano-ros root. It is loaded through
  `orchestration::sdk_index::SdkIndex::load` and read directly in
  `cmd/board_facts.rs` (`index_path`, two sites), `cmd/build.rs` (four
  `SdkIndex::load` / `index_path` sites) and `builder/preflight.rs`. It
  decides toolchain and SDK pins and, through preflight, whether an image is
  skipped.
- **The platform `nros-platform.toml` files** under `packages/platform/`.
  `BoardCatalog` resolves board descriptors against the platform families
  these files declare (`PlatformFamilies`, from the `nros-platform-config`
  crate), so an edit to one can change how a descriptor resolves.

Neither file is in the project walk, and neither loader goes through
`read_external`, so an edit to either leaves a configure stamp current over an
answer derived from the old bytes. Only `nros configure --force` (or any
change that does stale the stamp) picks it up.

Not measured as a user-visible failure yet: this was found by the sweep, not
by a broken build.

## Sweep that found it

```
git grep -n 'nros-sdk-index.toml"\|PLATFORM_CONFIG_FILENAME' -- packages/cli/nros-cli-core/src
```

Each hit was classified as a configure-path read of a file outside the
project, then checked for whether its bytes reach the stamp.

## Fix direction

Route both loaders' file reads through `configure_stamp::read_external`. For
the platform configs that means a read hook or a returned list of the files
`PlatformFamilies` read, because the loader lives in another crate that
cannot depend on `nros-cli-core`. The SDK index's in-crate reads can be
routed directly. Add a regression test per input in the style of
`configure_stamp::tests::an_external_read_is_a_stamp_input`.

## Fix

- `SdkIndex::load` reads the file through `configure_stamp::read_external`.
  Every in-crate load (`board_facts`, `build`, `build_next`, `preflight`,
  `setup::load_resolved_index`) goes through it, so routing the loader covers
  all of them at once. Outside a configure it is a plain read.
- `PlatformsTree` (in `nros-platform-config`) keeps the list of descriptor
  files it read, shadowed ones included, as `source_files()`. The new
  `configure_stamp::load_platforms_tree(roots)` loads the tree and, when a
  recorder is installed, records each of those files. `PlatformFamilies`
  (the board catalog), `model_ingest`'s executor-sizing ladder and
  `nros config` all load through it.
- `STAMP_FORMAT` 4 → 5. A format-4 stamp recorded neither input, so it is
  stale once.

Regression tests: `configure_stamp::tests::the_sdk_index_is_a_stamp_input`
and `configure_stamp::tests::a_platform_descriptor_is_a_stamp_input`. Each
edits the input after the configure, then removes it, and expects the stamp
to go stale both times. With its fix reverted, each fails.

Not covered, and tracked as open issue 2498: a NEW
`<root>/<name>/nros-platform.toml` added to a search root after a configure.
The stamp records the files that were read, not the directory listings, so a
new platform that shadows an existing name is not seen until something else
stales the stamp.
