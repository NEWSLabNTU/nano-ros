---
id: 1785
title: "`make-stall-watchdog.py` hangs in its own diagnostics when any process on the host holds an fd on a stuck FUSE mount, so the watchdog that must never hang never fires"
status: open
type: bug
area: [build, testing]
severity: medium
found: 2026-10-10
related: [issue-1403]
---

## What was measured

On 2026-10-10, on a shared host, `just check make-stall-watchdog` started
failing. Its source had not changed since `3c328827e2` (issue 1403), and the
same gate had passed twice on that host an hour earlier:

```
check-make-stall-watchdog: FAILED (issue 1403)
  - watchdog did not return within 40s — it never fired (stderr tail: '')
  - stall: rc -9, want 75
```

It failed solo too. The tier-2 fixture build stopped at its `check::fast`
preflight on it.

A `faulthandler` dump of the watchdog child, taken 15 s into the stall case,
puts it at `make-stall-watchdog.py:302` in `collect_diagnostics`. That is
`_pipe_fds(other)` in the loop over every process on the host. The child's
kernel wait channel was `request_wait_answer`, which is FUSE: a request to a
FUSE server that does not answer. The host carries several user FUSE mounts
(`fuse.sshfs`, `fuse.portal`, `fuse.gvfsd-fuse`).

`_pipe_fds` calls `os.stat()` on `/proc/<pid>/fd/<n>` for every fd of every
process. `stat` follows the link to the open file. For an fd on a FUSE file
whose server is stuck, that `stat` blocks with no timeout. The watchdog never
reaches `kill_tree` or its `NO VERDICT` line.

## Why it matters beyond this gate

The watchdog wraps every fixture and jobserver `make`. Its one job is to turn a
hang into a verdict (issue 1403). On a host where any other user's process holds
an fd on a stuck FUSE mount, the real watchdog hangs inside its diagnostics in
the same way. A jobserver stall then becomes an unbounded hang again, which is
the 1403 failure it exists to remove, now caused by a process the build does
not own.

## Direction

Identify a FIFO without following the link into another filesystem's server.
`readlink` gives `pipe:[<inode>]` for an anonymous pipe, so no `stat` is needed
for those. For a named FIFO, either stat only the paths the jobserver auth
names (`--jobserver-auth=fifo:<path>`), which are the only FIFOs the
diagnostic needs to match, or bound each `stat` with a timeout in a worker
thread. Acceptance: the stall case passes on a host where some unrelated
process holds an fd on a FUSE file whose server is suspended (`kill -STOP` on
an `sshfs` reproduces it on demand).
