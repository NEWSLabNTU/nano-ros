---
id: 1383
title: "`cargo_target_dir()` calls any hyphenated target dir a target TRIPLE, so
  a build script mirrors its generated header into the dir's PARENT — and the
  sanctioned scoped-dir helper always produces a hyphenated name"
status: resolved
resolved_in: fix/1401-1383-build-tooling — triple dir decided by `$TARGET`, not by a hyphen
type: bug
area: [build]
severity: medium
found: 2026-09-18
resolved: 2026-09-28
related: [issue-1382, issue-0400, issue-0616, issue-0111]
---

## What happens

`nros_sizes_build::cargo_target_dir()` resolves the root that
`write_header_to_target_dir` mirrors into. Order is: `CARGO_TARGET_DIR` env →
walk `$OUT_DIR` for a `build/` ancestor → `cargo metadata`.

The walk has to decide whether the component above the profile dir is the
target dir itself or a target-triple subdirectory, because `$OUT_DIR` is
`<target>/<triple>?/<profile>/build/<pkg>-<hash>/out` — the triple is optional.
It decides by asking whether the name **contains a hyphen**
(`packages/tooling/nros-sizes-build/src/lib.rs:1443`):

```rust
if name.contains('-') {
    if let Some(target) = triple_or_target.parent() {
        return Ok(target.to_path_buf());   // treat `name` as a TRIPLE
    }
} else {
    return Ok(triple_or_target.to_path_buf());
}
```

A target *directory* whose own basename contains a hyphen therefore reads as a
triple, and the function returns its **parent**.

## Why that is reachable, not theoretical

`nros_scoped_target_dir <suffix>` (`scripts/build/cargo.sh`, the sanctioned
spelling from issue 0400) is defined as
`printf '%s' "${CARGO_TARGET_DIR:-$PWD/target}-$1"` — it **always** appends a
hyphen. So every dir the helper produces (`target-param-services`,
`target-embedded`, …) trips the heuristic.

**Measured** while fixing issue 1382. Passing
`--target-dir "$(nros_scoped_target_dir param-services)"` to the
`check-compile-smoke` lane put the mirror at the **repo root**, creating
untracked `nros-c-generated/` and `nros-cpp-generated/` beside `packages/`:

```
$ git status --short
?? nros-c-generated/
?? nros-cpp-generated/
```

Walk for `OUT_DIR=<root>/target-param-services/debug/build/nros-c-<hash>/out`:
parent named `build` → profile dir `…/target-param-services/debug` →
`triple_or_target` = `…/target-param-services` → name contains `-` → return its
parent = `<root>`.

1382 worked around it by passing the dir as `CARGO_TARGET_DIR` instead, which
takes the first branch and never reaches the heuristic. That is correct for
that lane and leaves the heuristic in place for the next caller.

## Why it is quiet

Nothing fails. The header is written, just to a directory no consumer includes,
and the in-`$OUT_DIR` copy (phase-400 W5.c) is still correct — so the build
succeeds and the only symptom is untracked debris one `git status` away from
being swept into a commit by a blanket `git add`. A consumer that reads the
mirror instead of `$OUT_DIR` would get a stale header or none.

## What is NOT established

Whether any current in-tree caller other than 1382's reverted attempt reaches
the walk with a hyphenated target dir. `CARGO_TARGET_DIR` is set in most lanes,
which short-circuits before the heuristic; the exposure is a `--target-dir`
FLAG, which does not set the variable.

## Direction

The distinguishing fact is not the name — it is that a triple directory sits
INSIDE a cargo target dir, and cargo marks a target dir root with
`CACHEDIR.TAG`. Probing for that sibling answers the question directly instead
of guessing from punctuation. A narrower stopgap is to reject a candidate whose
name starts with `target`, which is the shape the helper emits.

Acceptance: a build script run under
`--target-dir $(nros_scoped_target_dir <x>)` must mirror its header inside that
dir, and `git status` must stay clean at the repo root.

## Resolution (2026-09-28)

The walk moved into `target_dir_from_out_dir(out_dir, target)`, which decides
by IDENTITY: the component above the profile dir is a triple dir exactly when
its name is `$TARGET` (the file stem, for a JSON spec path). Measured on cargo
1.98.1: a builtin triple builds into `<dir>/x86_64-unknown-linux-gnu/` with
`TARGET=x86_64-unknown-linux-gnu`; `--target ./my-custom.json` builds into
`<dir>/my-custom/` with `TARGET=my-custom`. A host build without `--target`
has `TARGET` set but no triple level, and the component is then the target
dir, whose name is not the triple.

### Why not `CACHEDIR.TAG` (the direction above)

Measured, it does not distinguish the two: cargo 1.98.1 writes `CACHEDIR.TAG`
at the target-dir root AND inside every `<triple>/` dir. And this repo's own
`target/` has none at its root at all (created by an older cargo), while its
`aarch64-unknown-none/` and `armv7a-nuttx-eabihf/` subdirs each do — so a
probe for the tag would have answered backwards on the main checkout.
`.rustc_info.json` is root-only but is an optional cache
(`CARGO_CACHE_RUSTC_INFO=0` suppresses it). `$TARGET` is what cargo itself
named the directory after, and every build script has it.

### Evidence

Unit test `target_dir_is_found_by_triple_identity_not_by_hyphen`: (a)
`target/debug/…`, (b) `target/<triple>/debug/…`, (c)
`target-param-services/debug/…`, (d) `target-param-services/<triple>/debug/…`,
plus a `--target <host>` build, a custom JSON target by name and by path, and
no-`build`-ancestor → `None`. Mutation (restoring the hyphen predicate) fails
case (c):

```text
assertion `left == right` failed: OUT_DIR=/w/target-param-services/debug/build/x-0123/out
  left: Some("/w")
 right: Some("/w/target-param-services")
```

Behavioural, a real build script calling `cargo_target_dir()` with
`--target-dir <scratch>/w1401/target-probe` and `CARGO_TARGET_DIR` unset:

```text
BEFORE  cargo_target_dir() = w1401                       <- the parent
        [--target x86_64-unknown-linux-gnu] = w1401/target-probe
AFTER   cargo_target_dir() = w1401/target-probe
        [--target x86_64-unknown-linux-gnu] = w1401/target-probe
```

Sweep for siblings: `git grep -nE "contains\('-'\)"` over the Rust tree finds
no other triple-vs-dir inference (the two other hits test a package name and an
`<os>-<arch>` host key), and no other build script walks `OUT_DIR` up to a
target dir; `nros-build-helpers`' two mirror writers reach this function and
are fixed by it.
