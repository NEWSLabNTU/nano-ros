---
id: 1829
title: "Since #1817, `nros sync` refuses the `local-msg-package` template's Rust leaf, whose `nros` dependency resolves only through a patch sync has not written yet, which stops the L3 lane at `generate-bindings`"
status: open
type: bug
severity: medium
area: [cli, orchestration, ci]
related: [1817, 1755]
found: 2026-10-11
---

## Measured

L3 queue job 114583597973 (merge_group run for #2001, on the queue's temporary merge commit,
runner `nano-ros-runner-newslab-server-241`, 2026-10-11 21:46Z) ran on the
rebuilt runner image. That image's doctor printed `[OK] user cache dir is writable`,
so issue 1755 is behind this lane now. The job then failed in `just ci matrix build`, at
`./scripts/regenerate-bindings.sh`:

```
  sync: examples/templates/local-msg-package
Error: rust_consumer::rust_consumer: cargo cannot resolve .../examples/templates/local-msg-package/src/rust_consumer/Cargo.toml:
error: no matching package named `nros` found
location searched: crates.io index
required by package `rust_consumer v0.1.0 (.../src/rust_consumer)`
This fails the image's own build too, so sync stops here rather than fall back to the SystemModel bound (issue 1817).
Location:
    nros-cli-core/src/orchestration/metadata_refresh.rs:249:21
error: recipe `generate-bindings` failed on line 4559 with exit code 1
```

`rust_consumer/Cargo.toml` names `nros`, `nros-board-linux` and
`nros-platform` as `version = "*"` crates.io dependencies. Its own comment says
these resolve through "the auto-managed `[patch.crates-io]` block written by
`nros sync`". The tracked manifest carries no such block, and no tracked
`.cargo/config.toml` patches them either (examples have no `.cargo/`, RFC-0098 D1).
So the leaf resolves only after sync has written its patch.

## What it is

An ordering defect exposed by #1817's fix (dbb9be70cb), not caused by the
template. Before that fix, the metadata refresh's cargo failure on this leaf
was swallowed and sync printed `done.`. Now it is fatal, and the refresh
evidently asks cargo to resolve the leaf before the in-repo patch rows that
would make `nros` resolvable are in place, or without them.

#1817's refusal is right for a leaf that cannot resolve ON ANY ROAD, which was
its measured case (a board crate missing a feature). It is wrong for a leaf
that resolves once sync's own patch rows are applied.

## What it is not

- Not issue 1755: the runner cache check passed in this job.
- Not the board-feature case #1817 measured: the template's dependencies exist
  in-tree, under the same names.
- Not a runner or image problem: the error is cargo's resolver over a tracked
  manifest.
- Undiagnosed for want of a local repro: whether the template, or every
  single-package cargo leaf that relies on sync's patch, is affected.
  `examples/templates/local-msg-package` is the first leaf
  `regenerate-bindings.sh` reaches after the mixed template.

## What would close it

The metadata refresh resolves a leaf with the same `--config`/patch rows the
leaf's build uses (or runs after they are written). Proven by `just ci matrix build`
reaching past `generate-bindings` on a self-hosted runner. #1817's refusal must
still fire for its own measured case: a manifest that cannot resolve with the
patch rows applied.
