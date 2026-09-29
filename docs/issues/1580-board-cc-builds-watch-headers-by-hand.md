---
id: 1580
title: "Board crates' own cc-rs compiles watch their sources by hand, blind to headers outside those paths"
status: open
type: tech-debt
area: [build, boards]
severity: low
found: 2026-09-29
related: [issue-1570, issue-0475, issue-0491]
---

## What is left

Issue 1570 gave the NuttX image lane a compiler-measured rebuild edge.
`nros_cc_flags::header_deps::{track_header_deps, emit_header_deps}` passes
`-MMD` to the compiler, then replays each `.d` file as
`cargo:rerun-if-changed`, so cargo watches exactly the files the compiler
read. `check-cc-header-deps` requires that helper pair on every compile that
consumes an env `*SOURCES*` list.

The board crates' OWN cc-rs builds are outside that gate. Each one watches its
inputs by hand, per file or per directory:

- FreeRTOS kernel, lwIP and glue;
- ThreadX kernel, NetX and glue;
- threadx-linux;
- mps2 lan9118.

A header those builds include from outside the watched paths can therefore
change without recompiling the object that includes it. That is 1570's class,
at lower stakes: these sources are vendored or in-crate, and they rarely
change underneath a build.

## Direction

Apply the same helper pair to each board cc-rs build. The hand-written
`rerun-if-changed` lines it makes redundant can then go, after checking
whether any of them also watches a non-compiled input. Then widen
`check-cc-header-deps` to every `cc::Build::compile` in a build script, or
record why a site is exempt.

## Acceptance

For each board crate: `touch` a header its C code includes from outside the
watched directories, and the board crate rebuilds. A no-op rebuild stays a
no-op.
