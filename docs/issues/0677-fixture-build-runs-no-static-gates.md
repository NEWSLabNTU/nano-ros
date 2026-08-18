---
id: 677
title: "`build-test-fixtures` runs none of the static gates that protect the compilation it is about to do, so a 23-second lane's finding is discovered by a multi-hour build instead"
status: open
type: tech-debt
severity: medium
area: build, testing
related: [issue-0319, issue-0532, issue-0674, phase-318, rfc-0061]
---

## What happened

`626505310` (#0532 item 5) retired
`nros_platform_time_since_epoch_{secs,nanos}` in favour of one
`nros_platform_time_now_ns`, and left `packages/api/nros-c/src/platform.rs`
calling the retired pair. Every embedded image that links `nros-c` then failed
at LINK time:

```
ld.bfd: libnros_c.a(...): in function `nros_c::platform::get_system_time_ns':
packages/api/nros-c/src/platform.rs:76: undefined reference to
    `nros_platform_time_since_epoch_secs'
```

**A gate for exactly this already existed, already named both symbols, and was
already failing.** `scripts/check-retired-platform-clock-symbols.py` carries
them in its `RETIRED` list; run by hand on the unmodified tree it exits 1 and
prints the offending file and line.

It is wired into `check-fast` (justfile:467), so `just check` — and therefore
`just ci` and `just ci-matrix` — would have caught it.

## The wiring gap

`build-test-fixtures` does have preconditions, and they are the RIGHT KIND of
thing:

```
build-test-fixtures lane="all": _require-build-sources _clear-fixture-stamp \
    generate-bindings setup-launch-resolve build-zenoh-posix-fixture \
    (build-test-fixtures-leaves lane)
build-test-fixtures-leaves lane="all": _require-leaf-includes
```

Every one of those asks *"is the environment ready to build?"*. None asks
*"is the tree in a state where building is meaningful?"* — so no static gate
runs before the compile.

The consequence is a cost asymmetry that is entirely avoidable. `check-fast`
is documented in the justfile as running **green in 23 s on a pristine
detached worktree**; the tier-2 fixture build is a multi-hour, multi-platform
compile. On this tree the 23-second lane knew the answer and the multi-hour
build is what actually reported it — twice, because the first rebuild was
consumed by an unrelated failure on another platform (issue 0674) before this
one surfaced.

## Why "just run `ci` first" is not the answer

That is what the tier ladder already says (RFC-0061), and it does not close
this: `just ci` runs `check-tier-preconditions` and then `check`, but the
FIXTURES must already be fresh for `test-all` to mean anything, so the honest
order a contributor follows is **build fixtures, then run the tier** — which
puts the expensive step first. `ci-matrix` is the same shape one lane over:
`_lane-gate tier2` gates fixture COORDINATES, not source validity.

So the ordering that makes gates cheap is exactly the ordering the workflow
discourages. Nothing here is a missing gate; it is a missing edge.

## Same shape as issue 0319

[Issue 0319](archived/) was a backend suite that existed and was never invoked
by `just check`, and a red sat on main for two days. This is the same failure
one layer down: **the gate is not missing, the EDGE to it is** — and an
uninvoked gate reads as coverage to everyone downstream of it.

## Direction

1. **Put the cheap static gates in front of the expensive build.** Give
   `build-test-fixtures` a dependency on the source-validity subset of
   `check-fast` — the gates that read only tracked sources and need no CLI, no
   `nros sync` and no provisioned toolchain, which is the subset `check-fast`
   already documents as pristine-worktree-safe. A build that cannot produce a
   valid artifact should not spend hours proving it.
2. **Keep the subset honest.** `check-fast` is fast *because* it is
   environment-free; adding an environment-dependent gate to it would make the
   new edge expensive and it would be removed again. Whatever subset is chosen
   needs the same "verify against a pristine detached worktree" rule the
   justfile already states for `check-fast`.
3. **Decide whether this is a gate or a warning.** A hard failure protects the
   hours; a warning keeps a contributor able to rebuild fixtures while
   knowingly mid-refactor. The tree's own precedent (`check-tier-preconditions`
   reports every unmet precondition at once rather than one per attempt,
   issue 0466) argues for reporting ALL findings up front and failing.

## Not this issue

The `nros-c` defect itself is fixed. This issue is only about the missing edge
that let a link error, rather than a 23-second gate, be the thing that reported
it.
