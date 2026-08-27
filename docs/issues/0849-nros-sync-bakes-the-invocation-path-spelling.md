---
id: 849
title: "`nros sync` bakes the invocation's path SPELLING into every leaf patch
  table, so working through a symlink to the checkout makes cargo see two copies
  of every core crate and refuse the build on `links`"
status: open
type: bug
area: cli
related: [issue-0616, issue-0457, issue-0463]
---

## Problem

`/home/aeon/data` is a symlink to `/mnt/wd`, so this checkout has two valid
absolute spellings:

```
/mnt/wd/data/projects/nano-ros                 (realpath)
/home/aeon/data/data/projects/nano-ros         (via the symlink)
```

Run anything that invokes `nros sync` with the CWD under the second spelling and
every leaf's `.cargo/config.toml` is rewritten with paths that climb out to the
filesystem root and back down through the symlink:

```toml
[patch.crates-io]
nros-board-linux = { path = "../../../../../../../../../home/aeon/data/data/projects/nano-ros/packages/boards/nros-board-linux" }  # nros-managed
```

The central `nros-patch.toml` is written from `NROS_REPO_DIR`, which
`activate.sh` sets to the realpath, so it is spelled `/mnt/wd/...`. The two
halves of the same patch set then disagree, and cargo resolves the SAME crate
twice under the two spellings:

```
error: failed to select a version for `nros-node`.
    ... required by package `nros v0.5.0 (/home/aeon/data/data/projects/nano-ros/packages/api/nros)`
    ... which satisfies path dependency `nros` of package `nros-board-linux v0.5.0 (/home/aeon/data/data/...)`
package `nros-node` links to the native library `nros_node`, but it conflicts
with a previous package which links to `nros_node` as well:
package `nros-node v0.5.0 (/mnt/wd/data/projects/nano-ros/packages/core/nros-node)`
```

`nros-node` carries `links = "nros_node"`, so cargo refuses outright rather than
building two copies. Every Rust fixture leaf fails at resolution; the native
fixture lane cannot build at all.

Same root as **#0616** one layer up: cargo keys a unit on the path SPELLING a
crate was reached by, not on the inode. There it cost duplicate `#[global_allocator]`
units in a shared `--target-dir`; here it costs the whole resolution.

## Why it is not a one-time mistake

It re-poisons itself. `just build-test-fixtures` calls `nros sync` on the
talker/listener leaves (`ensure_native_rust_generated` in `just/native.just`), so
one build from the aliased CWD undoes a hand-repair of all 22 leaves. Observed
three times in one session: sweep clean, run the build, sweep dirty again.

The tell is subtle — the failing build prints a `tmp/build-test-fixtures-*` path
under `/home/aeon/data/...` while every other path in the same output reads
`/mnt/wd/...`, and nothing says the two are the same directory.

`generated/*/Cargo.toml` carries the same spelling and is NOT rewritten by a
plain re-sync (the codegen stamp reads current), so a leaf can have a repaired
`config.toml` and stale generated manifests.

## Workaround

Invoke everything with the CWD at the realpath. Repair after the fact with:

```bash
cd /mnt/wd/data/projects/nano-ros
for f in $(grep -rl /home/aeon/data/data/projects/nano-ros --include=config.toml examples/ packages/); do
    NROS_REPO_DIR=/mnt/wd/data/projects/nano-ros nros sync "${f%/.cargo/config.toml}"
done
```

## Direction

Two things, and the first is the actual fix:

1. **`nros sync` should canonicalize the repo root before writing any path.**
   It already knows the realpath — `NROS_REPO_DIR` is set to it by
   `activate.sh` — but the leaf rows are computed from the invocation instead.
   In-repo rows are supposed to be relative and identical in every checkout
   (CLAUDE.md, issues 0457/0463); a `../../../../../../../../../home/...` row is
   neither, and only looks relative.
2. **A gate**, because this is silent until a build fails with an error that
   names neither sync nor symlinks. `check-cargo-config-tracked` already reads
   these files; the natural place for "no `[patch]` row escapes the checkout"
   and "every in-repo row is relative".

Worth checking whether the same spelling reaches anything else derived from the
invocation path — the `build/fixtures-build-make/*.mk` files and the codegen
stamps both looked suspect while chasing this.
