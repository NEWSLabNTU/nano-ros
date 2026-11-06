---
id: 2297
title: "`nros configure` reports a leaf up to date when its `generated/` tree is gone, so a codegen-stamp wipe is never refilled"
status: open
type: bug
area: [cli, build]
severity: low
found: 2026-11-06
related: [phase-535, 2296]
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
