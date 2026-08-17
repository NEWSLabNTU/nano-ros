---
id: 661
title: "Something wrote a NO-`--target` `nros_core` compilation into the mixed
  workspace tree, and `check-artifact-identity-budget` has been red since"
status: open
type: bug
area: build
related: [issue-0616, issue-0499, issue-0647, phase-340]
---

## Symptom

`just check` fails at `check-artifact-identity-budget`:

```
nros_core has 6 distinct -C metadata identities in
examples/workspaces/mixed/build-workspace-fixtures (budget 4, recorded
2026-08-07 by phase-340 W4)
```

Four of the six sit under an `x86_64-unknown-linux-gnu/` component. **Two do
not:**

```
ce582821337f04ad  .../cargo/nano-ros_1147c/nros-relwithdebinfo/deps/libnros_core-….rlib
a7c20e922b9bb82d  .../cargo/nros_ws_runtime_f22c6/nros-relwithdebinfo/deps/libnros_core-….rlib
```

A path with no `<triple>` component means the cargo invocation that produced it
passed **no `--target`**. Per phase-340 W3 (and CLAUDE.md's rule) *every* cargo
command cmake emits passes `--target`, host included, precisely because
`--target <host>` and no `--target` are different `-C metadata` identities that
share nothing. `check-cargo-target-spelling` passes, so whatever wrote these is
not one of the sites that gate reads.

## Why it is not pre-2026-08-08 residue

The gate's own remedy text says a no-`<triple>` path can be old residue. Here it
cannot be:

* `examples/workspaces/mixed/build-workspace-fixtures` was deleted wholesale and
  rebuilt on 2026-08-16, after which the same gate reported **4/4 identities,
  worst crate 5/5** — green, and the count was measured on a fresh tree.
* Both no-triple rlibs carry mtime **2026-08-17 03:03 local**. They were written
  after that rebuild, by something running that morning.

So this is a live writer, not history.

## What is NOT known

Which writer. Several agent sessions were building in this worktree that morning
(commits land at 02:15, 03:23, 04:26, 05:16, 05:45, 06:39, 07:16, 10:43), and
nothing in the artifact records who produced it. Deliberately not guessed.

Do NOT clear the tree to make the gate green: the gate says so itself, and the
two rlibs are the only evidence of the writer that exists. Find the invocation
first.

## Why it hides

`check-artifact-identity-budget` only measures crates THIS build rebuilt
(issue 0647). `nros_core` is rebuilt by very few changes, so most runs report
"not rebuilt … counts ALL rlib(s) in the tree", and the failure then looks like
tree accumulation rather than a specific bad write. It stays red for everyone
until someone reads the paths.

## Relationship to 0616

Same family. 0616 is "a cargo `--target-dir` serves exactly ONE workspace root",
where the `-C metadata` identity includes the path spelling a crate was reached
by. This is the sibling axis — the identity also includes whether `--target` was
passed at all — and it produces the same outcome: two compilations of one crate,
identical in features and profile, that cannot be shared.

## Reproduce

```
just check-artifact-identity-budget
ls -la examples/workspaces/mixed/build-workspace-fixtures/cargo/*/nros-relwithdebinfo/deps/libnros_core-*.rlib
```

The second command listing anything at all is the defect: that directory level
should not exist.


## Measurement 2026-08-17 — the premise needs correcting, and it is not residue

Two findings, from a tree that was deleted and rebuilt from scratch.

**1. Not residue.** `examples/workspaces/mixed/build-workspace-fixtures` was
`rm -rf`'d and rebuilt with `workspace-fixtures-build.sh linux mixed` while
clearing an unrelated identity-budget failure. The rebuilt tree still contains
two no-`<triple>` `nros_core` rlibs, written by that rebuild:

```
cargo/nros_ws_runtime_14eac/nros-relwithdebinfo/deps/libnros_core-08606ac517592b5c.rlib   2026-08-16 14:17:51
cargo/nano-ros_0b88c/nros-relwithdebinfo/deps/libnros_core-569a6970bf1fb59f.rlib          2026-08-16 14:17:37
```

So "delete and rebuild before believing the count" — which the gate's own
message prescribes, and which is the right first move — does NOT clear these.
A current build path produces them.

**2. The inference "no `<triple>` component ⇒ the invocation passed no
`--target`" does not hold.** In a `--target` build, cargo splits artifacts by
where they RUN: host artifacts (build-script executables, proc-macros, and the
dependency chain each needs) go to `<dir>/<profile>/`, target artifacts to
`<dir>/<triple>/<profile>/`. The no-triple directory here has exactly that
shape:

```
cargo/nano-ros_0b88c/nros-relwithdebinfo/
  deps/libnros_macros-*.so   deps/libserde_derive-*.so   deps/libpaste-*.so
  build/   (46 entries, incl. nros-c, nros-cpp, nros-node)
```

Proc-macro `.so`s and build-script dirs are host-only artifacts. Their presence
means this directory IS the host side of a `--target` invocation, not the output
of a `--target`-less one. Phase-340 W3's rule and
`check-cargo-target-spelling` are about the FLAG; this path shape is cargo's
own layout and would appear even with the flag always passed.

### What that leaves open

The question is no longer "who forgot `--target`" but **why `nros_core` is
needed host-side at all**. Nothing declares it as a `[build-dependencies]` entry
(`grep` over `packages/**/Cargo.toml` finds none), and `nros-macros` — the
proc-macro in the graph — deps only `syn`/`quote`/`proc-macro2`/`toml` plus
`nros-pkg-index` and `nros-launch-parser`. So the route is not obvious from the
manifests and wants tracing with `cargo tree --target` rather than inferred.

Stated rather than guessed: I did not establish that route, and one observation
cuts against the tidy version of this story — the TRIPLE directory also holds 16
`build/` entries and one proc-macro `.so`. Some of that is normal (a build
script's `out/` lands target-side while its executable lands host-side), but I
have not confirmed all of it, so a second invocation cannot be ruled out on this
evidence alone.

**Consequence for the budget either way:** if these are legitimate host-side
compilations, the budget of 4 counts two things it should probably separate —
host and target identities of the same crate — and the R3 axis the gate already
prints (`identities 154/54 (host/target)`) suggests it can tell them apart.
