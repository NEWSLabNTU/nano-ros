---
id: 920
title: "An interrupted codegen leaves a ZERO-BYTE generated file, and the compile error that follows names no leaf"
status: open
type: bug
area: cli, codegen
related: [rfc-0023]
---

## What happens

`rosidl-bindgen`'s `write_if_changed` (`packages/cli/rosidl-bindgen/src/generator.rs:33`)
ends in `std::fs::write`, which TRUNCATES the target and then writes it. Kill
codegen — or a build that drives it — between those two steps and the file is
left at zero bytes. It is a perfectly valid file, so nothing notices until
something tries to compile it.

Found for real: `just ci-l1` failed in `check-examples` with

    error[E0432]: unresolved import `duration::Duration`
     --> generated/builtin_interfaces/src/msg/mod.rs:4:9

and on disk:

    -rw-r--r-- 1 aeon users    0 Aug 30 11:28 duration.rs
    -rw-r--r-- 1 aeon users  110 Aug 29 22:58 mod.rs
    -rw-r--r-- 1 aeon users 2247 Aug 29 22:58 time.rs

One file truncated, its siblings from the previous day untouched. The cause was
a `check-examples` run killed mid-flight earlier in the session.

## Why it costs more than it looks

**It self-heals, which is why it has never been filed.** The next codegen sees
`existing != new` and rewrites. `nros sync` in the leaf fixed it in one command.
So the state is transient — but it survives long enough to red-line a lane, and
the person who hits it has no idea that re-running codegen is the remedy.

**The error names no leaf.** `check-examples` fans 37 units out over the
jobserver, each compiling with its own cwd, so the diagnostic is a RELATIVE path
(`generated/builtin_interfaces/...`) with no unit name anywhere near it. There
are 202 generated `builtin_interfaces` trees in this repo. Locating the one bad
file took a scripted sweep over all of them:

    for d in $(find examples packages -type d -name builtin_interfaces -path '*generated*'); do
        f="$d/src/msg/duration.rs"
        [ -f "$f" ] && ! grep -q 'struct Duration' "$f" && echo "STALE: $d"
    done

That is the real cost: a self-healing five-second problem presenting as a
twenty-minute hunt.

## Fix

Write to a sibling temporary and `rename` onto the target. `rename(2)` is atomic
within a filesystem, so an interrupted run leaves either the old file or the new
one, never a truncated one. It composes with the existing idempotent-skip — keep
the `read`-and-compare fast path exactly as it is, and only change the write
underneath it.

The same shape applies anywhere else in the CLI that writes a file a build later
reads; `write_if_changed` is the one on the codegen path and the one that bit.

## Acceptance

* Interrupting codegen mid-write leaves the previous file intact, verified by
  killing a run in a loop and checking that no generated file is ever zero bytes.
* The idempotent-skip still holds: an unchanged regeneration does not bump any
  mtime (that is what the helper exists for — cmake's mtime-driven rebuilds).
* Optional but worth it: `check-examples` names the failing UNIT alongside the
  compiler's relative path, so the next such error is attributable without a
  sweep.
