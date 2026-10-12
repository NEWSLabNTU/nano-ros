---
id: 1837
title: "A threadx-linux Rust link once failed on an undefined `nros_platform_threadx_pool_min_ever_free_bytes` and passed on an unchanged rebuild — a suspected missing dependency edge, seen once"
status: open
type: bug
area: [build, threadx]
severity: low
found: 2026-10-12
related: [1305]
---

## Seen

While migrating the ThreadX Rust leaves onto the selection package (issue 1305
phase C, PR #2029), the first threadx-linux build after `THREADX_DIR` was set
in that worktree failed to LINK:

```
undefined reference to `nros_platform_threadx_pool_min_ever_free_bytes'
```

The next build, with no source or configuration change, linked and ran (the
threadx-linux talker/listener pair then exchanged 19/19 messages).

The symbol is defined in C, `packages/platform/nros-platform-threadx/src/platform.c:366`,
and declared `extern` and called from Rust in
`packages/boards/nros-board-threadx/src/entry.rs:292` (the boot-time pool
report). So the Rust side can link before the C object that defines it is in
the archive the link reads.

## What it probably is, and what it is not yet

A pass on an UNCHANGED rebuild is the shape of a missing dependency edge or an
ordering race (CLAUDE.md: "if you wiped and it worked, you have a lead — not a
diagnosis"). Here nothing was wiped, which makes it more likely an ordering
race than a stale artifact. That is still a guess: it was seen ONCE, the
failing build's tree was not kept, and the first build after a `THREADX_DIR`
change is also the first build of that platform's C sources in that worktree.

## To close

Reproduce first: a fresh worktree, `THREADX_DIR` set, one
`bash scripts/build/fixtures-build.sh threadx-linux rust` under the usual
parallelism, repeated N times from a clean target for that row only. If it
reproduces, find the edge (`cargo build -vv`: does `nros-board-threadx`'s link
see the archive `nros-platform-threadx`'s build script produces, and is it
ordered after it?). If 10 clean runs pass, close it as not reproduced, with the
count.
