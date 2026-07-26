---
id: 272
title: "nros sync's `include = [nros-patch.toml]` is silently dead on stable cargo — external consumers get 'no matching package named nros'"
status: open  # premise disproven — see Verification (2026-07-26); needs reporter repro
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

## Verification (2026-07-26) — the stated premise does NOT reproduce

Cargo stabilized the config `include` key in **1.93**; on the toolchain this
repo pins (cargo 1.96.0) `-Z config-include` is explicitly refused as
unnecessary:

```
warning: flag `-Z config-include` has been stabilized in the 1.93 release,
  and is no longer necessary
  The `include` config key is now always available
```

Two probes on **stable** cargo 1.96.0:

1. `include = ["extra.toml"]` → `[env]` from the included file reaches
   `build.rs` (`NROS_INCLUDE_PROBE=Ok("yes")`).
2. The exact failing shape — a consumer whose ONLY source of
   `[patch.crates-io] nros = { path = … }` is an included central file:
   `cargo +stable build` resolves the patch and compiles the path crate.
   No `no matching package named 'nros'`.

So `nros sync`'s option-E include mechanism is sound on stable, and the
"silently dead include" diagnosis is wrong. The autoware_sentinel failure
was real but has a different cause. Most likely candidates, in order:

1. **Relative include path resolved from the wrong base.** Cargo resolves a
   relative `include` against the DIRECTORY OF THE CONFIG FILE. `nros sync`
   writes the entry relative to the leaf's `.cargo/`; an external consumer
   whose patch authority sits at a different depth (or whose config was
   hand-copied between dirs) gets a path that silently misses — cargo does
   not error on a missing include target.
2. **Stale or absent `nros-patch.toml`.** It is gitignored, carries ABSOLUTE
   paths, and dies on any checkout move — its own header says re-run sync.
   An external consumer that never ran `nros sync` against ITS workspace (or
   ran it before moving the checkout) has no valid central file.
3. **Toolchain older than 1.93** in the consumer's environment (the pin the
   consumer uses, not this repo's).

**Landed (2026-07-26):** a fail-loud reachability check in `nros sync` — if
the include target it just wrote is not readable from the leaf's `.cargo/`,
sync errors with the re-run recipe instead of emitting a config whose patch
cargo will silently drop. (An absolute include was tried and reverted: the
in-tree example configs are COMMITTED, so a host-absolute path would break
every other checkout. Relative stays; absolute is the fallback only when no
relative path exists — a consumer on a different filesystem root.)

**Next step (needs the reporter):** capture, from the failing
autoware_sentinel tree, (a) `cargo --version`, (b) the consumer's
`.cargo/config.toml` verbatim, (c) whether
`<nano-ros>/nros-patch.toml` exists and its paths resolve, (d)
`cargo config get patch --merged` on nightly (shows what cargo actually
merged). Fix direction stays open pending that: if (1), sync should write an
ABSOLUTE include path (matching the file's absolute-path convention) rather
than inlining the trio; if (2), sync should fail loud when the central file
it just wrote is unreachable from a leaf.
