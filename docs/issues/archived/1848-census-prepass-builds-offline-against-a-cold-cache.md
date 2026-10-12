---
id: 1848
title: "The census prepass builds its host image `--offline` against a cache nothing warmed, so a cross-only runner dies on `failed to download`"
status: resolved
type: bug
area: [ci, build, testing]
severity: medium
found: 2026-10-13
related: [0967, 1419, 1811]
---

## Measured

The live-peer board job "rows whose board is NOT this runner" ran on `main`
at `199f34afbd` (run 38251106285). It failed in "Build the fixtures those
rows resolve" twice in a row, on reruns 2 and 3:

```
census-prepass: examples/workspaces/features demo_bringup:zephyr_rust_qos
entity-census take demo_bringup:zephyr_rust_qos: taking the census with `demo_bringup:native_rust_qos` (native_rust_qos_entry)
error: failed to download `cmake v0.1.58`
Caused by:
  attempting to make an HTTP request, but --frozen was specified
nros build: `cargo build --manifest-path build/posix-zenoh/native_rust_qos_entry/Cargo.toml --config build/posix-zenoh/native_rust_qos_entry/nros-cargo.toml --frozen` exited 101
```

The headline reads like a network flake, but it repeated. The job never
reached the Zephyr 3.7 `build-cortex-m-c-talker-zenoh` row, which is the row
whose SDK selection issue 1811 fixed. So this defect stood between that fix
and its proof.

## Cause

`scripts/build/census-prepass.sh` runs `nros ws entity-census take --image <q>
--offline` for every cross workspace image whose model has a contract.
`take` builds the native sibling image with `nros build --offline`, which
passes cargo `--frozen`. A frozen build can only use crates that are already
in the cache.

`workspace-fixtures-build.sh` warms that cache before each of its own offline
builds with `nros_warm_leaf_cache` (issue 0967), but the prepass had no warm
step. On a runner whose rows are all cross images, nothing else has fetched
the host image's graph, so the prepass is the first thing to need it. On a
developer machine the cache is warm, so the defect never shows there. That is
the same blind spot issue 0967 described.

Reproduced locally against an empty `CARGO_HOME`, after moving the existing
census aside so that `take` has to build:

```
error: failed to download `ahash v0.8.12`
  attempting to make an HTTP request, but --frozen was specified
```

## Fix

- `nros ws entity-census take --plan` makes the same decision as `take` and
  builds nothing. It prints `producer <bringup>:<image>` for the host image
  it would build, or only the reason there is nothing to take. The CLI owns
  that choice (`census_producer_for`), so the script asks rather than
  re-deriving it.
- `nros_warm_leaf_cache` moved to `scripts/build/leaf-cache.sh`, sourced by
  both `workspace-fixtures-build.sh` and `census-prepass.sh`, so there is one
  copy.
- Before each `take --offline`, the prepass asks `--plan` and warms the
  producer's cache.

The build itself stays `--frozen`. Only the warm step reaches the network,
as issue 0967 requires.

## Verified

- `census-prepass.sh --id workspace-zephyr-rs-qos --only images` against an
  empty `CARGO_HOME` passes and writes `rust_qos_model.census.json`.
- The same `take --offline` without the warm step, against another empty
  `CARGO_HOME`, fails with the error above.
