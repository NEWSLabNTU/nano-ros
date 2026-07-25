---
id: 272
title: "nros sync's `include = [nros-patch.toml]` is silently dead on stable cargo — external consumers get 'no matching package named nros'"
status: open
type: bug
severity: medium
area: cli
---

## Finding (autoware_sentinel phase-14 pin bump, 2026-07-25)

`nros sync` writes the consumer's `.cargo/config.toml` with

```toml
include = ["../../nano-ros/nros-patch.toml"]
```

as the sole source of the nros/nros-core/nros-serdes `[patch.crates-io]`
rows. Cargo's config `include` key is still nightly-only
(`-Z config-include`); stable cargo (verified on 1.96.0) ignores the
line WITHOUT a warning, so an external consumer's first build fails
with:

```
error: no matching package named `nros` found
location searched: crates.io index
```

with no hint that the patch mechanism was dropped. In-tree example
workspaces dodge this because their crates carry direct `path =` deps
into the checkout; a colcon-mode external consumer (patch authority =
its own workspace root) has nothing else.

## Workaround shipped in autoware_sentinel

Hand-maintained trio rows appended to the same `[patch.crates-io]`
table (sync preserves rows it does not manage):

```toml
nros = { path = "../nano-ros/packages/core/nros" }
nros-core = { path = "../nano-ros/packages/core/nros-core" }
nros-serdes = { path = "../nano-ros/packages/core/nros-serdes" }
```

## Fix direction

Have `nros sync` inline the trio rows directly (marked `# nros-managed`
like the msg rows) when the resolved toolchain is stable, or at minimum
emit a loud post-sync warning that the include line needs nightly.
