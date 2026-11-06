---
id: 2297
title: "`nros configure` reports a leaf up to date when its `generated/` tree is gone, so a codegen-stamp wipe is never refilled"
status: resolved
type: bug
area: [cli, build]
severity: low
found: 2026-11-06
related: [phase-535, 2296]
resolved: 2026-11-07
resolved_in: "fix-issue-2297 branch: a workspace stamp sees a deleted generated/ crate"
---

## Measured

Seen while building phase-535 W4's tier-1 fixtures (`just
build-test-fixtures lane=tier1`), twice in a row.

- The fixture builder's row pre-pass (`scripts/build/fixtures-build.sh`
  `nros_presync_row_dirs`) calls `nros_codegen_stamp_check_or_wipe` on every
  row dir. After a CLI rebuild the codegen stamp has drifted, so it deletes
  `<dir>/generated/` and expects the `nros configure` that follows to rebuild
  it ("wiping (will regen)").
- For `packages/testing/nros-tests/bins/declarative-safety-listener`, that
  configure printed `configure: up to date (0 image(s))` and wrote nothing.
  The leaf's `Cargo.toml` path-deps `generated/std_msgs`, so the linux stage
  failed with `failed to load manifest for dependency std_msgs`. The tier-2
  build hit the same failure on the same leaf.
- `nros configure --force .` regenerated the tree.

So the configure stamp judges currency from the inputs. It does not check
that the outputs it recorded still exist. Issue 2296 is a sibling: the stamp
missed a moved resolver pin, and #3194 fixed that. This is the outputs half.

## Fix so far

`nros_codegen_stamp_check_or_wipe` now also retires the leaf's
`build/nros/configured` stamps (not the lock) when it wipes `generated/`. The
stamps described the tree it just deleted, so the configure after the wipe
regenerates.

## Still open

The CLI: a stamp whose generated outputs are missing should not count as up
to date. Any other tool that deletes `generated/` hits this again.

## Resolution

The CLI half was two cases, and both are closed now.

- **A package-only leaf** (a `package.xml` and no `system.toml`, which is what
  `declarative-safety-listener` is) is stamped under the project key, and
  `missing_output` returned nothing for that key. Commit 1ca8b3cc0a ("a
  package-only leaf's configure stamp sees a deleted generated/ crate") fixed
  that the same day: it asks `leaf_outputs_missing`, the check `nros build`
  refuses on. Its test is `deleted_generated_crate_is_stale_for_a_package_only_leaf`.
- **A workspace image** still answered "up to date". `missing_output` checked a
  workspace image's resolved model only, never the `generated/<pkg>` crates its
  members path-dep. Reproduced with a scaffolded Rust workspace
  (`nros new rws --board linux --lang rust`): `nros configure .`, then
  `rm -rf generated`, then `nros configure .` printed
  `configure: up to date (1 image(s))` and wrote nothing.
  `build::workspace_generated_missing` now reads every member's path-deps.
  The configure stamp asks it for a workspace image, so a crate under
  `<ws>/generated/` that has lost its `Cargo.toml` is stale. The same
  reproduction now names the missing crate and regenerates the tree:

  ```text
  configure: rws_node_bringup:linux: configure output missing: the generated message crate `std_msgs` (src/rws_node: ../../generated/std_msgs)
  ```

Test: `deleted_generated_crate_is_stale_for_a_workspace_image` in
`packages/cli/nros-cli-core/src/configure_stamp.rs`. It answered `Current`
after the delete before the change and answers `Stale` after it.

The fixture builder's own retirement of the `configured` stamps (the "Fix so
far" above) stays. It is now a second guard, not the only one.

