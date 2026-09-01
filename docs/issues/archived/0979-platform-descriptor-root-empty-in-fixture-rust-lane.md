---
id: 979
title: "The build-script platform root came from `current_dir()`, which is the
  CALLING PACKAGE — so every Rust fixture resolved its descriptors from a
  directory that does not exist"
status: resolved
type: bug
area: build, boards
related: [phase-400, issue-0978, issue-0491]
---

## Symptom

Every Rust fixture in the native lane dies in `nros-node`'s build script:

```
error: failed to run custom build command for `nros-node v0.5.0`
  --- stderr
  thread 'main' panicked at packages/boards/nros-board-common/src/platform_config.rs:299:33:
  NROS_PLATFORM_NAME=posix: unknown platform `posix`: no /posix/nros-platform.toml
```

`fixture-0000`, `-0004`, `-0008` and siblings — `examples/native/rust/{talker,
listener,lifecycle-node,custom-msg,service-*,action-*}`, `nros-tests/bins/entry-poc`.

The leading slash in `no /posix/nros-platform.toml` is the tell: the format
string is `no {root}/{name}/{PLATFORM_CONFIG_FILENAME}`, so `root` is the empty
string. The name in the message is what the CALLER asked for, not what is
missing — `posix` exists, at `packages/platform/nros-platform-posix/`.

## Reproduced in one line, and the original framing was wrong

The filing said "only under the fixture lane", and reasoned from the lane's
different `--target-dir`. That is not the variable. The variable is whether
`NROS_PLATFORM_NAME` is exported at all:

```
$ NROS_PLATFORM_NAME=posix cargo build -p nros-node
  thread 'main' panicked at platform_config.rs:299:33:
  NROS_PLATFORM_NAME=posix: unknown platform `posix`: no /posix/nros-platform.toml

$ NROS_PLATFORMS_DIR=$PWD/packages/platform NROS_PLATFORM_NAME=posix cargo build -p nros-node
  Finished `dev` profile
```

A bare leaf build "succeeded" because `BuildRungs::from_build_env()` returns
`None` when the variable is unset and the caller's own front-end and defaults
decide — not because the lane's environment was doing anything special. The
fixture lane is simply the only thing that exports it (`nros ws board-facts` →
`corrosion_set_env_vars`). No fixture build is needed to see this, and no
bisect was needed either.

## Root cause: a build script's cwd is its own package

```rust
let search = PlatformsTree::default_search_path(
    &std::env::current_dir().unwrap_or_default(),   // <- treated as the repo root
    std::env::var("NROS_PLATFORMS_DIR").ok().as_deref(),
);
```

`default_search_path` appends `packages/platform` and `config` to the root it is
given. Cargo runs a build script with cwd set to the package's own directory, so
the roots searched were `packages/core/nros-node/packages/platform` and
`packages/core/nros-node/config`. Neither exists. `load_search_path` skipped
both, returned `PlatformsTree::default()` — whose `root` is an empty `PathBuf`
— and the first lookup failed several frames later, naming a platform that is
present in the tree it never loaded.

**`2ee18cdaf` was the right suspect, and the diff says so more precisely than a
bisect would.** The code this replaced read:

```rust
let search = pc::PlatformsTree::default_search_path(
    &nros_build_paths::repo_root(),
    ...
);
```

`nros-node/build.rs` resolved the root correctly. When phase-400 W6 moved its
copy of the env-pointer dance into `nros-board-common` — the right move, and the
commit says why — the root resolution moved with it, and had to become something
`nros-board-common` could reach. `current_dir()` was reachable and wrong.

`nros-build-paths` was in fact reachable the whole time: it is already an
optional dependency of `nros-board-common`, enabled by the same `build-helpers`
feature that `platform_config` itself lives behind.

## Fix

**One:** `build_search_path()` resolves the root with
`nros_build_paths::try_repo_root()`, which walks up from `CARGO_MANIFEST_DIR`
looking for the `nros-sdk-index.toml` sentinel — the repo's one spelling of
"where is the repo", and the same one the old `nros-node/build.rs` used.
`try_` rather than the panicking form: an out-of-tree consumer has no sentinel
to find and must reach the `NROS_PLATFORMS_DIR` arm instead of dying on the
walk.

**Two, the Direction §2 half:** `load_search_path` now returns
`ConfigError::NoSearchRoot` when NO root in the path exists, naming every path
it tried. "Missing roots are skipped" stays true and stays right — a porter
prepends their own tree and the in-tree roots may or may not be there — but ALL
of them missing is a wrong ROOT, not a search path doing its job, and returning
an empty tree is what deferred the failure into a message about the wrong thing.

This improves the other three callers too. `nros-zpico-build` panics on the
error instead of silently loading an empty tree and falling back to builtins —
the silent-fallback failure phase-400 W1 records for the same file. `nros config`
already renders the error. `model_ingest` already treats a failure as `None`.

## Sweep

`current_dir()` used as a repo root, across every build-script-reachable crate:
this was the only site. The one other hit is
`nros-cli-core/src/cmd/build.rs:129`, where cwd IS the answer — it resolves the
user's workspace root for a CLI the user ran there.

## Tests

Three, in `platform_config`'s own module, needing no env and no cwd fiddling —
cargo runs a test binary from its package directory exactly as it runs a build
script, so `packages/boards/nros-board-common` IS the reproduction:

* `the_build_search_path_is_rooted_at_the_repo_not_the_calling_package` — the
  regression. The path must contain an existing root, and specifically the
  in-tree `packages/platform`.
* `a_search_path_with_no_existing_root_is_an_error_naming_what_was_tried`.
* `a_search_path_keeps_skipping_individual_missing_roots` — the negative
  control for the tightening, so the porter case cannot be broken by it.

Proven non-vacuous: restoring `current_dir()` fails exactly the first, and the
other 39 tests in the crate pass.

## Left open

Three walk-ups to `nros-sdk-index.toml` now exist —
`nros_build_paths::try_repo_root` (from `CARGO_MANIFEST_DIR`),
`nros-cli-core/src/cmd/config.rs::find_repo_root` and the inline loop in
`orchestration/model_ingest.rs` (both from `current_dir()`). They agree today
and each is correct for its caller: a build script has `CARGO_MANIFEST_DIR` and
no meaningful cwd, a CLI has the reverse. Unifying them needs a resolver that
takes its start point as an argument, which is a small design decision rather
than part of this repair — and `nros-build-paths`'s own doc comment already
names this drift ("a private `project_root()` in `qemu.rs` was deleted for
exactly this reason").

## Acceptance

* [x] A root that resolves empty fails where it is resolved, naming what it
      tried to resolve from — not several frames later as a platform lookup for
      a platform that does exist.
* [x] The Rust build script resolves its descriptors: the one-line reproduction
      above goes from panic to `Finished`.
