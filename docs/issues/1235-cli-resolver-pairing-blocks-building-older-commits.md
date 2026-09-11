---
id: 1235
title: "`nros sync` refuses a correctly paired resolver on older commits, so the
  board image cannot be built across a bisect"
status: open
type: bug
area: build
severity: high
related: [issue-0409, issue-0419, issue-0561, issue-1177, issue-1227, phase-424]
---

## Symptom

Checking out an older nano-ros commit and building the safety-island image
fails in the CLI's `sync` step (the consumer's board-build recipe calls it),
every time:

```
Error: sync: `.../packages/cli/nros-launch-resolve/target/release/nros-launch-resolve`
was built from play_launch db4af878230f but this `nros` was built from 01cb87d8f077.
```

`01cb87d8f077` is not a play_launch sha. It is the NANO-ROS COMMIT under test,
to the character. Measured at three commits, each time equal to that commit:

| nano-ros commit | pin recorded in `NROS_PLAY_LAUNCH_SHA` | real pin at that commit |
| --- | --- | --- |
| `01cb87d8f` | `01cb87d8f077` | `db4af878230f` |
| `0368d4040` | `0368d4040819` | `db4af878230f` |
| `4d6e0da9c` | `4d6e0da9cc52` | `db4af878230f` |

The resolver's `db4af878230f` is CORRECT -- it is what
`git ls-tree <commit> packages/cli/third-party/play_launch` reports for every
one of them. The resolver is properly paired; the value it is compared against
is wrong.

## Not the known case, and not yet explained

`play_launch_pin` already guards the failure mode issue 0419 recorded -- an
uninitialised submodule is an empty directory that EXISTS, and
`git -C <empty dir> rev-parse HEAD` walks up to the superproject:

```rust
fn play_launch_pin(root: &Path) -> Option<String> {
    let dir = root.join(PLAY_LAUNCH_DIR);
    if !dir.join(".git").exists() { return None; }
    ...
```

That guard is present IN THE FAILING COMMITS -- checked with
`git show <sha>:packages/cli/nros-cli-core/src/source_stamp.rs`. And the
harness that hit this ran
`git submodule update --init --checkout packages/cli/third-party/play_launch`
before every build, with `git -C .../play_launch rev-parse HEAD` confirming
`db4af878230f` afterwards. So the superproject-walk explanation does not fit
what was observed, and the real mechanism is UNKNOWN. Recording the evidence
rather than a guess.

## The remedies do not work

Three sanctioned commands were tried, none clears it:

* `just setup-launch-resolve` -- no-ops when cargo sees no change, leaving the
  resolver paired to the previous commit's pin;
* `just setup-cli` -- likewise, and separately cannot repair a STALE stamp:

      $ just setup-cli && ./packages/cli/target/release/nros source-stamp
      Error: source-stamp: STALE -- built from c5740a8ca4b9a942,
             sources are now 2bcfeee733de1b75.
      Rebuild: ./scripts/bootstrap.sh   (contributors: just setup-cli)

  The remedy names the command that just ran and did nothing. `rm -rf
  packages/cli/target/release/.fingerprint` clears it at some commits;

* `cargo clean --release --manifest-path packages/cli/Cargo.toml` followed by
  `just setup-cli` -- does NOT clear the pairing mismatch above.

## What it costs

Issue 1227 (the island image overflows DTCM by 45040 bytes on `main`) was
bisected to a 20-commit range and then blocked here: the two endpoints build,
and commits inside the range do not, for this reason rather than for anything
in their diffs. A regression in the one image that would catch it cannot be
attributed without building that image at each commit.

This is issue 1177's story from the contributor's end. No merge-gating lane
builds this image, and a contributor who tries by hand meets a tool that
refuses a correct setup and prints a remedy that no-ops.

## A third thing initialising the submodule breaks

With `play_launch` checked out, `check-box-sync-covers-tracked-source` goes red
on pristine `main` -- verified by re-running it with this branch's changes
removed:

```
check-box-sync-covers-tracked-source: 2 TRACKED path(s) would not reach the box mirror
  packages/cli/third-party/play_launch/src/play_launch_container/build/.built_by
      excluded by  --exclude 'build/'
  packages/cli/third-party/play_launch/src/play_launch_container/build/play_launch_container/colcon_test.rc
```

`play_launch` tracks a `build/` directory of its own, and the mirror's
build-output exclusion reaches into it. So the gate is red for anyone who has
initialised the submodule -- which is everyone who has built the CLI -- and
green only for a checkout that has not. A gate whose verdict depends on which
optional submodules you happen to have initialised cannot mean what it says.

Scoping the `build/` exclusion so it does not descend into
`packages/cli/third-party/` looks right, but that script is under active change
(`main`'s tip is another fix to it), so this is recorded rather than fixed here.

## Fix direction

1. Find why `NROS_PLAY_LAUNCH_SHA` takes the superproject's HEAD despite the
   `.git` guard. A `git -C` that resolves through a submodule `.git` FILE to a
   gitdir under the parent's `modules/` is the obvious next place to look;
   `nano-ros` is itself a submodule here, so `play_launch` is nested two deep,
   and that is the configuration this was not tested in.
2. Whatever the cause, a pin that comes out equal to the enclosing repo's HEAD
   is never right. Refuse to build rather than bake it -- a wrong pin is not a
   degraded answer, it is a false accusation against a correct setup, and it is
   unfalsifiable from the error message alone.
3. `setup-cli` and `setup-launch-resolve` must be able to repair what their own
   errors tell the reader to repair. Today both no-op when cargo sees no change,
   while the value they are supposed to refresh is one cargo does not track.

## The order that does work today

For anyone who needs to build an older commit before this is fixed:

    git submodule update --init --checkout packages/cli/third-party/play_launch
    rm -rf packages/cli/nros-launch-resolve/target/release/.fingerprint
    just setup-launch-resolve
    cargo clean --release --manifest-path packages/cli/Cargo.toml
    just setup-cli          # LAST -- setup-launch-resolve re-pins play_launch,
                            # which lives under packages/cli and invalidates a
                            # CLI stamp built before it
    ./packages/cli/target/release/nros source-stamp    # must say: fresh

That gets the stamp fresh. It does NOT fix the pairing mismatch above.
