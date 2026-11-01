---
id: 1785
title: "`make-stall-watchdog.py` hangs in its own diagnostics when any process on the host holds an fd on a stuck FUSE mount, so the watchdog that must never hang never fires"
status: resolved
type: bug
area: [build, testing]
severity: medium
found: 2026-10-10
related: [issue-1403, phase-505]
resolved_in: "issue 1773 (no foreign stat, bounded scan) + phase-505 W3 (a deadline on the whole diagnostics phase)"
---

**Home: [phase-505](../../roadmap/archived/phase-505-ci-lanes-reach-and-infra-robustness.md)** — homed 2026-10-17; listed in that phase's "Issues homed here (2026-10-17)".

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

## Resolution (2026-10-19, phase-505 W3)

**Most of this was already fixed under
[#1773](1773-stall-watchdog-host-scan-blocks-on-hung-fuse.md).** That issue was
filed the same day from the same measurement (the same `rc -9, want 75` and the
same `request_wait_answer`). After its fix, `_pipe_fds` keys a foreign
process's pipes by their `readlink` text and never `stat`s a foreign fd, and
the scan of other processes runs in a forked child with a budget
(`NROS_JOBSERVER_SCAN_SECS`). Its `scan-hang` and `no-follow` cases in
`check-make-stall-watchdog` fail against the watchdog from before 1773. This
issue is the same report, filed twice.

**What was left, and is now closed: the rest of the diagnostics phase had no
deadline.** Make's OWN fds are still `stat`ed, because that is the only way to
recognise a named FIFO. Make's `/proc` entries, `ls -l` and `gdb` also run
before the verdict. `_bounded_diagnostics` now runs all of
`collect_diagnostics` in a forked child with one deadline
(`NROS_JOBSERVER_DIAG_SECS`, default: the scan budget + 120 s), and the child
streams each line as it writes it. When the deadline passes, the child is
SIGKILLed. The report keeps every line written before the stuck step and says
`DIAGNOSTICS INCOMPLETE … the last line written was …`, and `kill_tree` and
`NO VERDICT` (rc 75) happen anyway.

**A reproducer that needs no root:** `NROS_JOBSERVER_DIAG_FAULT=hang`
(test-only) makes the stat of make's own fds block forever, the way a stat on a
hung FUSE file does. The new `diag-hang` case in `check-make-stall-watchdog`
expects rc 75, the NO VERDICT line, `DIAGNOSTICS INCOMPLETE: no answer within
2s`, the partial report, a return in under 30 s, and no survivor. Run against
the unbounded call (`collect_diagnostics` in place of `_bounded_diagnostics`),
it gets rc -9, no NO VERDICT line, and hits the gate's 40 s ceiling, which is
the failure this issue describes. The whole gate takes 18 s.
