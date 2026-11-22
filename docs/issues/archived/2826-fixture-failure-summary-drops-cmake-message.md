---
id: 2826
title: "A fixture stage's failure summary quotes `CMake Error at <file>:<line>` and drops the message under it"
status: resolved
type: bug
severity: low
area: [ci, fixtures, build]
related: [issue-1158, issue-2193, issue-2827]
found: 2026-11-22
resolved: 2026-11-22
resolved_in: "fix(build, issue 2826): a CMake error's failure summary carries its message"
---

## What happened

Tier 1 of run 44874748326 failed configuring `connect_facts_ws`, and the job
log's summary was all anyone saw:

```
first error line(s) in .../linux.log:
  3222:CMake Error at .../cmake/NanoRosEntityInventory.cmake:676 (message):
  3233:  Error:
  3267:Error: configure failed: `cmake -S build/posix-zenoh-linux-cpp_bringup ...
```

It names where, never what. The cause (issue 2827) was in the indented block
under line 3222, and had to be fetched from the run's `tier1-junit` artifact.
For a while it read as unreproducible, because the summary was taken for the
whole evidence.

## Why

`scripts/build/log-first-errors.sh` prints the lines that match its error
pattern, and nothing else. A CMake `message(FATAL_ERROR)` prints a `CMake
Error at <file>:<line> (message):` header and then the message, indented,
over several lines. No line of the message matches a pattern of its own,
except a bare `Error:` when the message embeds an eyre report.

## Fix

Each `CMake Error` line now brings the indented block under it into the
summary as context (`N-` lines, like `grep -A`): up to 16 lines, skipping
blank lines, and ending at the first line that is not indented. A context
line does not use up the summary's line budget.

Selftest case 11 is this run's shape: the header, the message, a bare
`Error:` followed by the CLI's numbered report, the remedy line, and the
`Configuring incomplete` line, which must stay out. Mutation-checked: with
the block off, the selftest fails, printing exactly the two lines the job log
printed.

The full stage log was already kept: `fixture-tree.sh reports` uploads
`tmp/build-test-fixtures-*/` in the `tier1-junit` and `post-submit-junit`
artifacts (14 days). Nothing new is uploaded.
