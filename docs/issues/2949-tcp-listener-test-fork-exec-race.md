---
id: 2949
title: "`a_listener_this_process_holds_is_ours...` reads its child between fork and exec, where the child still holds the listener, and fails as `Ours`"
status: open
type: bug
severity: low
area: [testing]
related: []
found: 2026-11-24
---
## What
`nros-tests` `tcp_listener::tests::a_listener_this_process_holds_is_ours_and_another_pid_does_not_hold_it`
failed once in a `just ci gate` run (2026-11-24, branch
`fix/2853-zenoh-frag-from-declared-sizes`, a change that does not touch it):

```text
panicked at packages/testing/nros-tests/src/tcp_listener.rs:171:9:
assertion `left == right` failed
  left: Ours
 right: Other
```

## Why
The negative control spawns `sleep 5` and immediately asks `holder(child.id(),
port)`. Rust's `TcpListener` is `O_CLOEXEC`, which closes the socket in the
child at `exec`, not at `fork`. Between the two, the child's `/proc/<pid>/fd`
still holds the listener's inode, so a check that lands in that window reads
`Ours`. Under a loaded gate host the window is wide enough to hit.

## Fix direction
Wait until the child has exec'd before checking. For example, poll until
`/proc/<pid>/exe` names `sleep`, or until its fd table no longer holds the
inode, with a bound. That fixes the start race; a retry would not.

## Acceptance
The test asks only after the child has exec'd, and fails if the child
genuinely still holds the socket.

## The fork/exec window does not reach this check (measured 2026-11-25)

Host 118, rustc 1.99.0, `main` at `b3ffe8a739`. The stated mechanism needs the
check to run while the child is between `fork` and `exec`. On Linux,
`Command::spawn` returns only after the child's `exec` succeeded: through
`posix_spawn` (glibc uses `CLONE_VFORK`, so the parent resumes at the exec),
or through fork+exec, where the parent blocks on a `CLOEXEC` error pipe until
the exec closes it. So by the time `holder(child.id(), port)` runs, the
listener (`SOCK_CLOEXEC`) is already gone from the child's fd table.

Measured with the test as it is on `main`:

| Run | Failures |
| --- | --- |
| the test alone, 300 times | 0 |
| the test alone, 400 times, 16 at once | 0 |
| the whole `nros-tests` lib suite, 60 times at `--test-threads 32` (other tests spawning beside it) | 0 from this test |

A wait for the child's exec (`/proc/<pid>/exe` differing from the test
binary's) was written and passes, but it waits for something `spawn` already
guarantees, so it is not the fix and was not landed.

What is still unexplained is the original `left: Ours`. For `Ours`, a process
in `tree(sleep_pid)` must hold a fd whose inode is one of the port's LISTEN
rows, and `sleep` has no children. The next red should be read with that in
hand: log `tree(child.id())` and each member's matching fd when the negative
control fails, before choosing a fix. Kept open, severity low.

## Diagnostics on the next red (2026-11-28)

The negative control's failure message now carries what `holder` saw,
read before the child is reaped (`describe_holder` in `tcp_listener.rs`):
the port's LISTEN inodes, and every member of `tree(child.id())` with its
ppid, `comm`, `exe` and each fd that links to one of those inodes. The test
also points the diagnostic at its own process, which does hold the
listener, and asserts that it names the socket fd, so the diagnostic is
known to work before it is needed.

No fix is landed: the stated fork/exec cause was measured false (above),
and nothing in the code explains `Ours` from a `sleep` with no children.
The issue stays open until a red with this evidence names the process and
fd that made it `Ours`.
