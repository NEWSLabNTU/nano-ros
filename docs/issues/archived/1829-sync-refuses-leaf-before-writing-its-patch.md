---
id: 1829
title: "Since #1817, `nros sync` refuses the `local-msg-package` template's Rust leaf, whose `nros` dependency resolves only through a patch sync has not written yet, which stops the L3 lane at `generate-bindings`"
status: resolved
type: bug
severity: medium
area: [cli, orchestration, ci]
related: [1817, 1755, 1603]
found: 2026-10-11
resolved_in: "fix/1829-sync-patch-before-resolve"
---

## Measured

L3 queue job 114583597973 (merge_group run for #2001, on the queue's temporary merge commit) failed
in `just ci matrix build` at `./scripts/regenerate-bindings.sh`:

```
  sync: examples/templates/local-msg-package
Error: rust_consumer::rust_consumer: cargo cannot resolve .../examples/templates/local-msg-package/src/rust_consumer/Cargo.toml:
error: no matching package named `nros` found
location searched: crates.io index
This fails the image's own build too, so sync stops here rather than fall back to the SystemModel bound (issue 1817).
```

It blocked every merge: the `queue` check runs this lane.

## Root cause

The title's guess was wrong. Sync DID write the leaf's patch before the probe:
`sync: wrote [patch.crates-io] → …/src/rust_consumer/.cargo/config.toml`
prints right before the refusal. The defect is WHERE the probe looks for patch
rows, not WHEN they are written.

`probe_patch_config` (`metadata_build.rs`, issue 1603) collected `[patch]` rows
from the `.cargo/config.toml` files on the HARNESS's ancestor chain only. The
harness lives at `<ws>/build/nros-metadata/metadata-probe/<id>`. In a colcon
workspace whose Rust leaf is its own cargo workspace (empty `[workspace]`, so
its patch authority is `<ws>/src/rust_consumer/`), the leaf's config is a
SIBLING of that chain, not an ancestor. The probe got no `--config`. It runs
from the nano-ros checkout (1603), so cargo never read the leaf's own config
either. The crates.io-named `nros` then searched crates.io.

The probe itself had failed this way since 1603. That failure was degraded
silently (issue 0288). #1817 added `component_resolution_error`, which used the
same `probe_patch_config`, so it reported the same wrong answer as the leaf's
fault. That is what made it fatal.

The bug reproduced locally on the first sync, with the leaf's config already on
disk, so this was never a clean-state or ordering effect. Single-package leaves
were unaffected because their harness sits under `<leaf>/build/`, so the leaf's
config IS on the harness chain. A cargo workspace with members was unaffected
too, because its authority is `<ws>`, which is also on the harness chain.

## Fix

`probe_patch_config(harness_dir, component_dir, cwd)` now walks the COMPONENT's
chain as well as the harness's. The patch authority is by definition an
ancestor of the component (`find_patch_authority`), so this reads the rows the
leaf's own build reads. Layers are merged component chain first, farthest to
nearest, then the harness-only directories. Both the probe (`build_metadata`)
and #1817's check use it. `component_resolution_error` now reports an
unreadable patch config (a missing `include`) as the cause, instead of dropping
it and reporting cargo's unpatched error.

Sweep: every cargo call in the CLI that resolves a leaf from a cwd outside that
leaf. The other `cargo metadata` / `MetadataCommand` sites (`nros_config.rs`,
`locate.rs`, `cargo-nano-ros`, `drift.rs`) run with the leaf as cwd or use
`--no-deps`. Only the two `metadata_build.rs` sites crossed the boundary, and
both go through the fixed function.

## Proof

- `nros sync examples/templates/local-msg-package` prints
  `source metadata — 1 rebuilt` and `sync: done.` (before: the 1817 refusal).
  The probe now really runs for this leaf, where before it degraded silently.
- A copy of the template with `nros` given a nonexistent feature still exits 1
  with cargo's `does not have that feature` and the 1817 message. Removing the
  feature gives exit 0. #1817's refusal still fires.
- Unit test `the_probe_takes_a_sibling_leafs_patch_rows`, which runs in
  `check-cli-tests` on the PR `CI` context. The full `generate-bindings` chain
  needs ROS ament packages and codegen output, which no pull-request lane
  builds. So the end-to-end check stays in L3.
