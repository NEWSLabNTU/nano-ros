---
id: 1383
title: "`cargo_target_dir()` calls any hyphenated target dir a target TRIPLE, so
  a build script mirrors its generated header into the dir's PARENT — and the
  sanctioned scoped-dir helper always produces a hyphenated name"
status: open
type: bug
area: [build]
severity: medium
found: 2026-09-18
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
