---
id: 1773
title: "The jobserver stall watchdog's scan of OTHER processes followed their fd
  links, so one process stuck on a hung FUSE mount anywhere on the host kept the
  watchdog from ever firing — and failed the `pre-push` hook for everyone"
status: resolved
type: bug
area: build, ci
severity: medium
found: 2026-10-10
related: [1403]
---
## Measured

2026-10-10, a shared host with ~20 interactive users. The `pre-push` hook's
`check-fast` failed on one gate, unrelated to the push (a docs-only diff), and
failed again solo:

```
check-make-stall-watchdog: FAILED (issue 1403)
  - watchdog did not return within 40s — it never fired (stderr tail: '')
  - stall: rc -9, want 75
```

The watchdog itself sat in `wchan = request_wait_answer` — a FUSE request with
no answer. Two other processes on the host were in the same state: `bfs -S dfs
/ -name …` searches another session had started an hour earlier, which had
walked into the gvfs / document-portal FUSE mounts under `/run/user` and stuck
there. Once those two exited the gate passed (14 s) with no change to the tree.

## Cause

On a stall, `collect_diagnostics` lists "other processes holding a
jobserver" by scanning EVERY pid on the host, and `_pipe_fds(other)` called
`os.stat("/proc/<other>/fd/<n>")` on each fd to find FIFOs. That stat FOLLOWS
the magic link and asks the file's own filesystem for its attributes; for an
fd open in a hung FUSE mount the request never returns. The scan is evidence,
but it runs BEFORE the verdict, so a stuck scan meant no `NO VERDICT` line, no
kill of the stalled make — the exact silent hang issue 1403 exists to end.
The gate measured it faithfully: its negative control is a real stall run
through the real scan.

## Fix

- **No foreign link is followed.** `_pipe_fds` keys a pipe by its LINK TEXT
  (`pipe:[<ino>]` for an anonymous pipe, the path for a named FIFO), which
  `readlink` produces from the dentry without calling the filesystem. Only
  make's OWN fds are still stat'ed (the only way to recognise a named FIFO at
  all); a foreign process's path links are matched against make's FIFO paths
  and the `GMfifo` name, by text.
- **The scan is bounded anyway.** Reading another process's `environ` /
  `cmdline` takes that process's mmap lock, which a process faulting on a
  hung FUSE mapping holds. The scan now runs in a forked child with a budget
  (`NROS_JOBSERVER_SCAN_SECS`, default 60), streaming its lines and a
  progress record per pid; on overrun the child is SIGKILLed (a FUSE wait is
  killable) and the report says `SCAN INCOMPLETE … stuck reading /proc/<pid>`.
  The verdict lands either way.

## Guard

`check-make-stall-watchdog` gains two cases, both FAILING against the pre-fix
watchdog: `scan-hang` (the scan is fault-injected to block — the verdict must
still arrive within budget, with the INCOMPLETE line) and `no-follow` (a
foreign process holding a regular file, a named FIFO and an anonymous pipe:
both pipes recognised, the file not, and `os.stat` called on none of them).

A real hung FUSE mount cannot be produced on demand without a FUSE server, so
the hang is fault-injected (`NROS_JOBSERVER_SCAN_FAULT=hang`, test-only); the
no-follow half is checked against the actual syscall boundary instead.

## Sweep

`git grep -nE "/proc/.../(fd|environ|cmdline)" -- scripts just packages/testing tests`
finds one other reader, `scripts/build/sample-build-leaves.sh`: a manual
profiling tool that reads `cwd`/`cmdline` of the BUILD's own descendants only,
never the host, and gates nothing.
