---
id: 979
title: "`just build native`'s Rust stage panics `unknown platform \\`posix\\`: no
  /posix/nros-platform.toml` — the descriptor root resolves EMPTY, and only under
  the fixture lane"
status: open
type: bug
area: build, boards
related: [phase-400, issue-0978]
---

## Symptom

Every Rust fixture in the native lane dies in `nros-node`'s build script:

```
error: failed to run custom build command for `nros-node v0.5.0`
Caused by:
  process didn't exit successfully: .../build/cargo-fixtures/linux/nros-relwithdebinfo/
    build/nros-node-91a014c71fff31f1/build-script-build (exit status: 101)
  --- stderr
  thread 'main' panicked at packages/boards/nros-board-common/src/platform_config.rs:299:33:
  NROS_PLATFORM_NAME=posix: unknown platform `posix`: no /posix/nros-platform.toml
```

`fixture-0000`, `-0004`, `-0008` and siblings — `examples/native/rust/{talker,
listener,lifecycle-node,custom-msg,service-*,action-*}`, `nros-tests/bins/entry-poc`.

The message is the tell: `no /posix/nros-platform.toml`, with a LEADING SLASH.
The format string is `no {root}/{name}/{PLATFORM_CONFIG_FILENAME}`
(`platform_config.rs:661`), so `root` is the empty string. The platform
descriptor tree was loaded from an empty search root, found nothing, and the
lookup for `posix` then failed — the name in the message is what the caller
asked for, not what is missing.

## It is lane-specific, which is the useful half

The same leaf builds fine on its own:

```
$ cd examples/native/rust/talker && cargo build
   (succeeds)
```

So this is not "the descriptor is missing from the tree". It is the fixture
lane's environment — a different `--target-dir`
(`build/cargo-fixtures/linux/nros-relwithdebinfo`) and whatever the lane does or
does not export — under which the root resolves empty. `PlatformsTree::
load_search_path` documents "missing roots are skipped, not fatal", which is
right for a search path and is also what turns a wrong root into an empty tree
instead of an error at the point the root is wrong.

## Suspect, NOT bisected

Both files in the panic — `packages/boards/nros-board-common/src/platform_config.rs`
and `packages/core/nros-node/build.rs` — were last touched by the same commit,
which landed on main earlier the same day:

```
2ee18cdaf refactor(phase-400 W6): one reader for the build rungs, and the census follows the code
```

Its own message describes exactly the risky shape:

> `nros-node/build.rs` grew its own copy of the env-pointer dance when the
> executor tenant landed […] so `nros-node` now delegates.

Consolidating two readers of the same env-pointer dance is precisely how one
caller's root resolution can start behaving like the other's. That is a
hypothesis with motive and opportunity, not a measurement: **no bisect was run**,
and the commit's own verification ("platform rung 11, env front-end 77, no lane
4") suggests it was checked through a build that was not this lane.

## How it was found

Not by looking for it. Issue 0978 was blocking the native lane at the C/C++
link stage; once that was fixed the lane got further than it had all day and
reached this. Earlier runs never saw it because they died first — which is worth
saying plainly: **this failure has no established start date.** It may predate
0978's symptom by any amount.

## Direction

1. Bisect across `2ee18cdaf` with the fixture lane, not a bare leaf build —
   the distinction above is the whole reproduction.
2. Wherever the root is resolved, an EMPTY root should be an error at that
   point, not a silently-skipped search entry. "Missing roots are skipped" is
   correct for a genuine search path and wrong for a root that was supposed to
   be computed and came back blank.

## Acceptance

* `just build native` completes its Rust fixture stage.
* A root that resolves empty fails where it is resolved, naming what it tried
  to resolve from — not several frames later as a platform lookup for a
  platform that does exist.
