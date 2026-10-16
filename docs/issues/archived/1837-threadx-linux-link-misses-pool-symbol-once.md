---
id: 1837
title: "A threadx-linux Rust link once failed on an undefined `nros_platform_threadx_pool_min_ever_free_bytes` and passed on an unchanged rebuild — a suspected missing dependency edge, seen once"
status: resolved  # not reproduced in 10 clean builds, 2026-10-16
type: bug
area: [build, threadx]
severity: low
found: 2026-10-12
resolved: 2026-10-16
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

## Closed: not reproduced (2026-10-16)

Ran the reproduction this issue asked for, on main at `613122dae3`, x86_64
host, 48 cores:

- 10 runs of `bash scripts/build/fixtures-build.sh threadx-linux rust`, each
  after deleting the shared group dir `build/cargo-fixtures/threadx-linux`
  (from `nros_fixture_row_artifact_dir`, so every run rebuilt the board, the
  platform port and the ThreadX kernel from source). All 9 rows of the family
  ran in parallel under the script's own makefile, which is the parallelism
  the original build had. 10 of 10 linked, with no `undefined reference` in
  any log. 213-230 s each.
- 5 more runs alternated `THREADX_DIR` set and unset without cleaning, to
  copy "the first build after `THREADX_DIR` was set". They are NOT evidence:
  the path set was the one `nros locate threadx` already resolves, and the
  build scripts watch the tree's content rather than the variable's text
  (issue 0491), so each run rebuilt nothing (4 s).

Why an ordering race is unlikely: the symbol's definition and its only caller
are in ONE crate. `nros-board-threadx`'s build script compiles `platform.c`
into `libnros_platform_threadx.a` with `+whole-archive`, and the caller is that
crate's own `src/entry.rs`. Cargo runs a crate's build script to completion
before compiling the crate, and the final link comes after every crate, so no
schedule links the caller before the archive exists. The function has existed
since `ad598928c4` (2026-09-18), well before the sighting, so a stale source
is unlikely too.

What is left unexplained: the sighting was in a worktree where `THREADX_DIR`
had just changed, and issue 1280 shows that a worktree's inherited path
variables used to point builds at ANOTHER checkout's sources. A first build
that compiled a different `platform.c` would fail this way and a second build
would not, if the variable was re-rooted in between. That would make this an
instance of 1280, which is fixed. Reopen with the failing build's tree kept if
it is seen again.
