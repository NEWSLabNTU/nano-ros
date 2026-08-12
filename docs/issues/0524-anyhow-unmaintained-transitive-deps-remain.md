---
id: 524
title: "`anyhow` is unmaintained: the first-party deps are gone, three transitive ones remain"
status: open
type: tech-debt
area: build
related: [issue-0523, rfc-0070]
---

## Context

`anyhow` is no longer maintained, and this tree standardises on `eyre`. A census
of every tracked manifest and lockfile (2026-08-12) found four sites, of which
two were first-party and are now removed.

## Removed — first-party, and both were DEAD

Neither needed porting to eyre, because neither was used:

| site | what it was |
| --- | --- |
| `packages/testing/nros-build-profile/Cargo.toml` | `anyhow = "1"` declared; **zero uses in its sources** |
| `packages/cli/Cargo.toml` `[workspace.dependencies]` | `anyhow = "1.0"`; **no member inherits it** (`packages/cli/Cargo.lock` never resolved it) |

The only two `anyhow` mentions in first-party Rust are a doc-comment example
(`nros-build/src/lib.rs`) and a prose comment
(`nros-platform/src/board/runtime.rs`) — neither compiles anything.

The root `Cargo.lock` diff for the removal is one line (the dependency edge),
which is the check that nothing was re-resolved on the way.

## Remaining — transitive, in two chains

`anyhow` still appears in four lockfiles because dependencies pull it:

| chain | reached from | ours? |
| --- | --- | --- |
| `play_launch_parser` -> `anyhow` | `packages/cli/nros-launch-resolve` | **a fork we pin** (`packages/cli/third-party/play_launch`, RFC-0060) |
| `wasip2` / `wasip3` -> `wit-bindgen` -> `wit-bindgen-rust{,-macro}` -> `wit-component`, `wasm-metadata`, `wit-bindgen-core` -> `anyhow` | root `Cargo.lock`, `bins/qos-override-pubsub`, `bins/ros2-string-interop` | no — upstream wasi tooling |

The wasi chain is not actionable here: it enters through `wasip2`/`wasip3`, which
are transitive standard-library-adjacent crates, and nothing in this repo chooses
`wit-bindgen` directly. It goes away when upstream moves, not when we do.

`play_launch_parser` is the actionable one. It is a fork this project pins and
whose branch this project lands fixes on (the vendored-fork workflow in
CLAUDE.md), so converting its `anyhow` usage to `eyre` is a change we can make
and push to the fork ourselves.

## Why this is filed rather than fixed

The first-party half was a two-line deletion and is done. The `play_launch`
half is a change to a vendored fork, which by repo policy the agent commits and
rebases locally while the maintainer pushes — so it wants to be a deliberate
piece of work rather than a drive-by.

## Acceptance

* `play_launch_parser` uses `eyre`, the fork branch is pushed, and the
  superproject pointer is bumped to the pushed commit (in that order).
* `git grep '^anyhow' -- '*.toml'` stays empty for first-party manifests, and no
  lockfile resolves `anyhow` except through the wasi chain.
* A note in this issue when the wasi chain drops it upstream, so the remaining
  entry is not mistaken for ours.
