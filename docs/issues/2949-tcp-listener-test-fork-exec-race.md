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
