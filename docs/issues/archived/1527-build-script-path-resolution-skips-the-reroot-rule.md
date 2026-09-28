---
id: 1527
title: "Five board build scripts resolved a path-valued SDK variable without issue
  1280's re-root rule — three with a raw `env::var` beside siblings that used the
  shared resolver, two behind a PRIVATE helper named like the shared one"
status: resolved
type: bug
area: build, boards
severity: medium
found: 2026-09-28
resolved: 2026-09-28
related: [1280, 1391, 1336, 0491, 0196]
---

## What happened

Issue 1280's rule has two halves. The shell half — `just/sdk-env.just` through
`scripts/lib/checkout-paths.sh` — is gated by `check-inherited-checkout-paths`
and holds: 21 path exports, all re-rooted, proven in both checkout shapes. The
BUILD SCRIPT half — "every `build.rs` through
`nros_build_paths::reroot_foreign`" — is gated by nothing, and five scripts had
drifted off it.

Measured on `main` at `e3de8bda1`, in an agent worktree
(`<main>/.claude/worktrees/<id>`, the shape CLAUDE.md prescribes for parallel
sessions):

```
$ echo $FREERTOS_CONFIG_DIR
/home/aeon/repos/nano-ros/packages/boards/nros-board-mps2-an385-freertos/config
$ pwd
/home/aeon/repos/nano-ros/.claude/worktrees/agent-a7859ba61b1c91799
```

The inherited value belongs to a **different** nano-ros checkout, and this one
carries its own copy of the same relative path. `reroot_foreign` exists to
answer exactly that: outside any checkout → KEEP, inside this one → KEEP,
inside a different one → RE-ROOT.

Five sites did not ask it:

| site | how |
| --- | --- |
| `nros-board-mps2-an385-freertos/build.rs` | raw `env::var("FREERTOS_CONFIG_DIR")` |
| `nros-board-mps3-an536-freertos/build.rs` | raw `env::var("FREERTOS_CONFIG_DIR")` |
| `nros-board-s32z270-freertos/build.rs` | raw `env::var("FREERTOS_CONFIG_DIR")` |
| `nros-board-freertos/build.rs` | a PRIVATE `fn env_path(name)` — `canonical(env::var(name))` |
| `nros-board-threadx-linux/build.rs` | a PRIVATE `fn env_path_or(name, default)` — same |

## Why it survived

**The three raw reads sit two lines under siblings that do it right.** In all
three the function reads:

```rust
let freertos_dir = nros_build_paths::freertos_dir();     // re-rooted
let lwip_dir = nros_build_paths::lwip_dir();             // re-rooted
let freertos_config_dir = env::var("FREERTOS_CONFIG_DIR")// NOT re-rooted
    .map(PathBuf::from)
    .unwrap_or_else(|_| config_dir.clone());
```

The shared accessor `nros_build_paths::freertos_config_dir()` exists, but it
defaults to the *mps2* board's `config/` for every caller, which is wrong for
the other two boards — so the site that needed a per-board default took the
whole resolution into its own hands instead of taking the one piece it needed
(`env_path`, which re-roots and has no default).

**The two private helpers are the worse shape, and the reason is not style.**
They are *named like* the shared resolver, so every call site reads as if the
rule applied. And they take the variable name as an **argument**, so no probe
that matches literals can see which variables lost the rule: the census in
`scripts/nros-build-wiring.py` classified `nros-board-threadx-linux` as
`workspace-relative` — resolving no SDK path at all — while it was resolving
two.

## The measurement, both directions

Before, in the worktree, `nros-board-threadx-linux` compiled silently. After
routing its helper through `nros_build_paths::env_path`, the same command says
what had been happening:

```
$ cargo check --manifest-path packages/boards/nros-board-threadx-linux/Cargo.toml --no-default-features
cargo::warning=nano-ros: $THREADX_DIR named another nano-ros checkout
  (/home/aeon/repos/nano-ros/third-party/threadx/kernel); building this one
  instead (/home/aeon/repos/nano-ros/.claude/worktrees/agent-.../third-party/threadx/kernel).
  A linked worktree inherits the parent shell's absolute SDK paths — issue 1280.
thread 'main' panicked at build.rs:71:5:
  ThreadX Linux port not found at <worktree>/third-party/threadx/kernel/ports/linux/gnu
```

The panic is the submodule not being checked out in this worktree, and it is
the **correct** outcome — the same one `just` already produces, because
`sdk-env.just` re-roots on its side. 1280's own words: a gate that RAN and
measured the wrong tree is worse than one that failed.

Census line, which is the before/after in one number
(`python3 scripts/nros-build-wiring.py --scripts`):

```
   5  resolving one WITHOUT it   ->   0  resolving one WITHOUT it
```

Nothing changes in a normal checkout: there `owner == here`, so `reroot_foreign`
returns the value unchanged and `env_path` canonicalises exactly as the private
helpers did.

## What was NOT changed, and why

`nros-board-threadx`'s `THREADX_EXTRA_INCLUDES` / `NETX_EXTRA_INCLUDES` are
COLON-SEPARATED LISTS of paths, and `nros_build_paths` has no list form — each
element is canonicalised and none is re-rooted. `nros-board-common`'s three raw
`NUTTX_DIR` reads are a second open site, with a real question attached (NuttX
is built IN PLACE, so re-rooting into a worktree names an unbuilt kernel rather
than a stale one). Both are phase-471 work items, not silent omissions.

## Fix

`nros_build_paths::env_path` at all five sites — it canonicalises (which is
what issue 0491 wanted from the private helpers) *and* applies 1280's rule, so
nothing is traded away. The `nros-board-freertos` submodule-presence probe was
moved onto the same resolver in the same commit: it read `FREERTOS_DIR` raw
while the compile below it read the re-rooted value, so the guard measured one
checkout and the build compiled another (issue 0196's shape).

## The structural gap this leaves

`check-inherited-checkout-paths` covers the shell half of 1280 and contains
zero references to `build.rs`, `nros_build_paths` or `env::var` — its reach is
narrower than the rule it enforces, which is the 0196 pattern the 2026-07-28
audit found in four gates. Until a gate covers the build-script half, this can
recur the same way. phase-471 W3 carries it; the census reports the number in
the meantime so a regression is at least visible.
