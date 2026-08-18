---
id: 684
title: "`check-image-panic-policy` enumerated by filesystem walk, so it read 637 untracked build-output files to find 139 tracked ones"
status: resolved
type: performance
area: testing
related: [issue-0618, phase-360, phase-366]
---

## Symptom

A gate whose own docstring says it is BUILDLESS took ~10 minutes on a cold page
cache, in `check-fast` — the lane whose stated premise is "~1 min, survives the
per-push cadence".

## Cause

It enumerated with `Path.glob("**")`:

```python
for main_rs in list(root.glob("examples/**/src/main.rs")) + list(
    root.glob("packages/testing/**/src/main.rs")
):
    if any(p in main_rs.parts for p in ("target", "build", "generated")):
        continue
```

`glob("**")` must DESCEND a tree to discover it, and those two roots contain
every cmake build dir, west workspace and `_deps/` checkout in the repo. The
filter then discards what the walk just paid for — and it is an EXACT component
match, so `build-zenoh` never matched `build`.

## Measured (warm cache; the cold run was ~600 s)

| | |
| --- | --- |
| `main.rs` found by the walk | 974 |
| kept after the filter | 776 |
| **of those, TRACKED** | **139** |
| untracked build output read | **637** |
| walk | 4.63 s |
| `git ls-files` | 0.00 s |

Where the 637 came from:

```
336  under a build-zenoh/ component
182  under a build-cyclonedds/ component
 84  under a build-xrce/ component
 14  under a build-workspace-fixtures-xrce/ component
```

and the most eloquent single path:

```
examples/qemu-riscv64-threadx/c/talker/build-zenoh/_deps/corrosion-src/test/hostbuild/hostbuild/src/main.rs
```

Corrosion's own test fixture, judged by a nano-ros panic-policy gate.

## What this was NOT

**The verdict was correct.** Of the 637 untracked files, zero contain an
`nros::main!` call, so none reached the count — before and after the fix the
gate reports the same `21 image(s) declare exactly one ending`. The exposure was
latent: any staged copy of a Rust entry, or any vendored source that happened to
match, would have been counted as an image, and per-RMW build dirs stage the
same entry three times.

Worth stating plainly because the first reading of this measurement — mine — was
"the count is not what it says". It was.

## Fix

Enumerate through the git index, which is the rule this tree already has:

* `feedback_no_find_use_git_ls_files` — avoid `find` for repo enumeration;
* phase-360 W3's `source-manifest.sh`, whose comment records the same lesson
  from the other direction: "it enumerates through the git index (so gitignored
  build trees can never leak in, which is what made the old find-based walk both
  slow and falsely stale)".

Extending the filter list was the wrong repair: a denylist against a set nobody
controls, still paying the traversal to build the list it then throws away.

Result: **4.63 s -> 0.037 s** warm, and the gate can no longer read a file that
is not ours.

## Provenance

Found 2026-08-19 while verifying this gate for issue 0618's closure — the run
was slow enough to be worth explaining rather than waiting out.
