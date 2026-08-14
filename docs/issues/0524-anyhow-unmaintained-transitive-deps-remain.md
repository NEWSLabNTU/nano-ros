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


## Decisions 2026-08-15 (phase-355 W3) — measured, and one of them is not what this issue expected

Re-census against the current tree. First-party manifests are still clean
(`git grep '^anyhow' -- '*.toml'` empty outside `third-party/`); four lockfiles
still resolve it, in the same two chains.

### Chain A — `play_launch_parser` → **REMOVE A DEAD DECLARATION**, not a port

This issue expected a port: "converting its `anyhow` usage to `eyre` is a change
we can make and push to the fork ourselves."

**There is no usage to convert.** `anyhow = "1.0"` sits in
`play_launch_parser/Cargo.toml:16` and appears NOWHERE in the crate — zero hits
across `src/` and `tests/`. Its errors are `thiserror` (`src/error.rs`). This is
the third instance of exactly the pattern this issue already deleted twice in
first-party manifests: a declared dependency nothing imports.

It IS compiled into our binary, so it is not free:

```
$ cargo tree -i anyhow --target all      # in packages/cli/nros-launch-resolve
anyhow v1.0.104
└── play_launch_parser v0.1.0 (…/third-party/play_launch/…)
    ├── nros-launch-resolve v0.5.0
    └── ros-launch-resolve v0.9.0 → nros-launch-resolve
```

Decision: **remove the line.** One deletion, no porting, no behaviour change.
The mechanics are the constraint, not the edit: it is a vendored fork, so per
CLAUDE.md the agent commits and rebases locally while the maintainer pushes, and
the fork branch must be pushed BEFORE the superproject pointer moves. It also
touches `packages/cli/nros-launch-resolve/Cargo.lock`, which must go through
`just lock-update` rather than a bare regeneration.

Not done here deliberately: this checkout's submodule sits one commit behind the
recorded pointer, and doing fork surgery from a stale detached HEAD is how a
confusing state gets created (it nearly did earlier in this same phase — see the
0507 postscript on stale remote-tracking refs).

### Chain B — the wasi / wit-bindgen family → **ACCEPT**, and the reason is stronger than "not ours"

The full path, measured rather than assumed:

```
cbindgen → tempfile → getrandom 0.4.2 → wasip2 / wasip3 → wit-bindgen
    → wit-bindgen-rust-macro → wit-bindgen-rust
    → {wit-component, wasm-metadata, wit-bindgen-core, wit-parser} → anyhow
```

So it enters through `cbindgen`, which this project very much does choose. But:

```
$ cargo tree -i anyhow --target x86_64-unknown-linux-gnu   → nothing to print
$ cargo tree -i anyhow --target all                        → nothing to print
```

`wit-bindgen` IS reachable; the crates beneath it that pull `anyhow` are behind
features that nothing enables. **`anyhow` is a lockfile entry that is never
compiled, for any target, in any configuration this workspace builds.**

That is the reason to accept, and it is worth stating precisely because "an
unmaintained crate in our lockfile" and "an unmaintained crate in our binaries"
are different risks. What an unmaintained `anyhow` would actually risk here is
nothing: no code path reaches it. It leaves when `getrandom` or the wasi tooling
moves, which is not ours to schedule.

### Summary

| chain | decision | why |
| --- | --- | --- |
| `play_launch_parser` | **remove** (dead declaration) | compiled into our binary, imported by nothing |
| wasi / wit-bindgen (3 lockfiles) | **accept** | never compiled for any target; lockfile-only |

## Acceptance

* `play_launch_parser` uses `eyre`, the fork branch is pushed, and the
  superproject pointer is bumped to the pushed commit (in that order).
* `git grep '^anyhow' -- '*.toml'` stays empty for first-party manifests, and no
  lockfile resolves `anyhow` except through the wasi chain.
* A note in this issue when the wasi chain drops it upstream, so the remaining
  entry is not mistaken for ours.
