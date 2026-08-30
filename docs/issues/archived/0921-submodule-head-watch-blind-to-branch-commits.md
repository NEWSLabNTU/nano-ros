---
id: 921
title: "Both build scripts watch the submodule's `HEAD` file, which does not move
  when you commit on a branch — so the stamped pin goes stale exactly while
  developing play_launch"
status: resolved
type: bug
area: build
related: [issue-0409, issue-0419, issue-0427, issue-0897, issue-0915]
---

## Problem

`packages/cli/nros-launch-resolve/build.rs` and
`packages/cli/nros-cli-core/build.rs` both bake the play_launch commit into
their binary, and both keep it fresh the same way:

```rust
let gitdir = p.join(rel);                 // from `.git`'s `gitdir: …`
println!("cargo:rerun-if-changed={}", gitdir.join("HEAD").display());
```

with the reasoning, verbatim:

> for a submodule `.git` holds `gitdir: …` and its CONTENT never changes when
> the submodule's HEAD moves — the move happens in `<gitdir>/HEAD`.

That is true for a **detached** submodule, which is the normal state: `HEAD`
holds the SHA, so a pin bump rewrites the file and the probe fires.

It is false when the submodule is **on a branch**. There `HEAD` holds
`ref: refs/heads/<branch>` and is CONSTANT across commits; the commit moves
`<gitdir>/refs/heads/<branch>` (or `packed-refs`). So the build script does
not re-run, the binary keeps the previous SHA, and the 0409 guard it feeds
compares a lie — the exact failure 0419's comment says it was written to
prevent.

**The blind case is the one that matters**: a submodule sits on a branch
precisely while someone is developing a play_launch change, which is when the
pin moves most often.

## Observed

While landing issue 0915 (two commits in play_launch, submodule on
`abi3-stable-api`):

```
$ just setup-launch-resolve        # rebuilt, reported success
$ nros sync examples/workspaces/rust
Error: sync: `…/nros-launch-resolve` was built from play_launch c532d40fe86d
       but this `nros` was built from caab6fbc4bd5.
```

`c532d40f` was the FIRST of the two commits. Deleting the binary and forcing a
full rebuild did not help — cargo had no reason to re-run the build script, so
the stale `NROS_PLAY_LAUNCH_SHA` was baked again. Detaching the submodule
(`git -C … checkout --detach <sha>`) fixed it immediately, which is the
diagnosis: `HEAD` became a SHA and the watch finally had something to see.

Two properties made it expensive to read:

- **The recipe reports success.** `setup-launch-resolve` prints `built: …`
  either way; the disagreement only surfaces later, in `nros sync`, naming
  neither the branch nor the reason.
- **The remedy the error prints does not work.** It says
  `just setup-launch-resolve`, which is what had just run.

## Direction

After reading `<gitdir>/HEAD`, follow it:

- if the content is `ref: <path>`, also
  `rerun-if-changed=<gitdir>/<path>` — the ref file the commit actually moves;
- watch `<gitdir>/packed-refs` too, since a packed ref has no loose file;
- and/or `<gitdir>/logs/HEAD`, which is appended on every commit AND checkout
  and is the single file that covers both shapes (present whenever reflogs are
  enabled, which is the default for a non-bare repo — so it is a good belt,
  not a sufficient brace).

**Fix both sites in one change.** They are two spellings of one rule and the
comment in each already points at the other ("Same fault as the CLI side; both
must agree on `unknown` or the 0409 guard compares two different things"). A
fix to one leaves the guard comparing a fresh pin against a stale one, which is
worse than both being stale.

Worth a test: the shape is checkable without a build — construct a scratch repo
with a submodule on a branch, commit in it, and assert the emitted
`rerun-if-changed` set names the moved ref.

## Fixed

One shared helper, `packages/cli/build-support/submodule_watch.rs`,
`include!`d by both build scripts — the repo's existing idiom for build-script
sharing (`nros-cli-core/build.rs` already `include!`s `src/source_stamp.rs`).
Both sites in one change, as the issue required: fixing one alone would leave
the 0409 guard comparing a fresh pin against a stale one, which is worse than
both being stale.

It watches the gitlink, `<gitdir>/HEAD`, **the ref `HEAD` points at**,
`packed-refs` (a packed ref has no loose file, so the loose watch alone would
be inert) and `logs/HEAD` (appended on commit and checkout; a belt, since
reflogs can be disabled). Paths are lexically normalised — `git submodule`
writes a relative `gitdir:`, so every path otherwise carries `..` hops into
the build log.

Proven end-to-end, not just by the watch set. With the submodule on a branch:

    git commit --allow-empty          # moves the ref and NOTHING else
    just setup-launch-resolve
    nros-launch-resolve --version  ->  play_launch 030f34a2…

An empty commit touches no file, so it is exactly the case the old watch could
not see; before this change the binary kept the previous sha. Rolling the
branch back re-stamped it again.

`tests/submodule_watch.rs` covers the four shapes: a branch (asserting the ref,
`packed-refs` and the reflog are named), a detached HEAD (still watched, and no
ref file invented), an uninitialised submodule (gitlink only, so
`--init` re-stamps), and a plain non-submodule repo.

Not addressed here, and worth its own look: `setup-launch-resolve` prints
`built:` whether or not it rebuilt, which is why this took a `nros sync`
failure two steps later to notice.
