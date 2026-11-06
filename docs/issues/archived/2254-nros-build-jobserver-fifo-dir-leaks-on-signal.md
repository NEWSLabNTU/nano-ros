---
id: 2254
title: "An interrupted `nros build` leaves its jobserver FIFO directory in the temp dir"
status: resolved
type: bug
severity: low
area: [cli, build]
related: [issue-2239, rfc-0117, phase-489]
found: 2026-11-05
resolved_in: "2026-11-06"
---

## What

`nros build` makes a fifo-form jobserver (RFC-0117 D4) in a private
directory, `std::env::temp_dir()/nros-jobserver-<pid>-<seq>/fifo`
(`packages/cli/nros-cli-core/src/builder/jobserver.rs`, `create_fifo`). The
directory is removed only by `impl Drop for Jobserver`. A signal that ends the
process (Ctrl-C, SIGTERM, SIGKILL) skips `Drop`, so the directory and its FIFO
stay behind.

Measured on 2026-11-05 while testing issue 2239: one Ctrl-C and one SIGKILL of
`check-template-copy-out` left `nros-jobserver-625846-0` and
`nros-jobserver-644324-0` in `$TMPDIR`. Passing runs left none.

Each one is a directory holding one FIFO, so the leak is small; but it grows
by one entry per interrupted build and never shrinks. That is the same
"nothing retires it" class issue 2239 fixed for the gates' scratch trees.

## Why it cannot just be unlinked early

A fifo-form client finds the pool by PATH (`--jobserver-auth=fifo:<path>`), so
the FIFO must exist for as long as any `make` child can open it. Unlinking it
right after `open` would break the clients the jobserver exists for.

## What would close it

Candidates, to choose by measurement:

- When creating a pool, retire any `nros-jobserver-<pid>-*` directory whose
  `<pid>` is not a live process (the orphan rule
  `scripts/build/gate-scratch.sh` applies, with the same pid-plus-start-time
  check so a recycled pid is not mistaken for the owner).
- Put the directory under the build's own `build/` instead of the temp dir,
  where the build tree's own lifecycle owns it.
- Remove it from a SIGINT/SIGTERM handler as well as from `Drop` (this does not
  cover SIGKILL, so it would still need one of the above).

Acceptance: interrupting `nros build` with Ctrl-C, SIGTERM and SIGKILL, then
running it again, leaves no `nros-jobserver-*` directory whose owner is gone.

## Resolution

Fixed with the first candidate, which is the only one that also covers
SIGKILL. Moving the directory under the build tree only moves the leak: a
killed build still leaves it there, so it needs the same sweep, and one pool
serves every image of a multi-image build, which has no single build tree to
own it. A signal handler alone misses SIGKILL.

- `create_fifo` names the directory `nros-jobserver-<pid>-<start>-<seq>`,
  where `<start>` is field 22 of `/proc/<pid>/stat` (read after the last `)`,
  the rule `scripts/build/gate-scratch.sh` uses). Without `/proc` the old
  `nros-jobserver-<pid>-<seq>` shape is written, and only the pid is checked.
- Before making a pool, `retire_orphan_fifo_dirs` walks the temp dir and
  retires a directory only when its name parses in one of those two shapes,
  its owner is provably gone (`kill(pid, 0)` is `ESRCH`, or the pid is alive
  with another start time), and it holds nothing but a FIFO named `fifo`. It
  uses `remove_file` + `remove_dir`, never a recursive delete. An owner that
  is alive or cannot be proved dead keeps its directory, so concurrent builds
  and the old-shape directory of a live pid are left alone; an old-shape
  directory of a dead pid is retired.
- Measuring the leak found a second path the issue did not name: Ctrl-C
  makes cargo exit 101, and `nros build` then leaves through
  `std::process::exit(child code)`, which runs no destructors. So every
  FAILED build leaked its directory too, signal or not. The pool is now
  dropped before that `exit`.

Measured on 2026-11-06 with `examples/linux/rust/talker`, `TMPDIR` set to an
empty directory outside any checkout, each build under `setsid` and signalled
3 s in through its process group (`kill -<sig> -- -<pgid>`), then one more
build run to completion:

| | after INT | after TERM | after KILL | after the next full run |
| --- | --- | --- | --- | --- |
| before (main) | 1 dir | 2 dirs | 3 dirs | 3 dirs |
| after | 0 | 1 (TERM's) | 1 (KILL's; TERM's retired) | 0 |

The fixed binary, run once over the three old-shape orphans the unfixed one
had left, retired all three.

Unit tests beside the existing jobserver tests
(`packages/cli/nros-cli-core/src/builder/jobserver.rs`) plant directories for
a dead pid (both name shapes), the test process itself, a recycled pid (live
pid, wrong start time), an old-shape name of a live pid, non-matching names, a
directory holding an extra file, one whose `fifo` is a regular file and an
empty one, and assert which survive. Replacing the start-time comparison with
`true` turns two of them red.
