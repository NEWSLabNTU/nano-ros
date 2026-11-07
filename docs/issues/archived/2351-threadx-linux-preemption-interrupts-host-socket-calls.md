---
id: 2351
title: "On threadx-linux, a host socket call the scheduler preempts returns EINTR, and the session open reports it as `ConnectionFailed`"
status: resolved
type: bug
area: [threadx, platform, network]
severity: medium
found: 2026-11-07
related: [issue-2094, phase-535]
resolved: 2026-11-07
resolved_in: "p535-quiet-link-router-evidence branch: nsos-netx finishes an interrupted call against the socket's own deadline"
---

## Symptom

`rtos_e2e test_rtos_quiet_link_keeps_its_session::platform_3_Platform__ThreadxLinux`
failed on the nightly in 2.9 s (it never reproduced on a developer host):

```text
[INFO] nros: boot config: domain 0 (baked), locator tcp/127.0.0.1:9000 (baked), ...
[ERROR] nros: [    0.010000] zpico Session -> ConnectionFailed (the two are indistinguishable downstream)
Executor::open failed: Transport(ConnectionFailed)
```

The router had answered the harness's readiness probe a moment earlier. 0.010 s
is one ThreadX tick.

## Cause

The ThreadX Linux port preempts a thread by sending it SIGUSR1
(`_tx_linux_thread_suspend`), and `_tx_linux_thread_init` installs that
handler with `sa_flags = 0`, i.e. without `SA_RESTART`. A ThreadX thread that
is blocked in a host system call when the scheduler preempts it gets -1/EINTR.
The board's network is `stack = "host"`: `nsos-netx` forwards every
`nx_bsd_*` call to the host socket, and none of those wrappers handled EINTR,
so an interrupted call read as a failed one.

Measured, with the fixture image unchanged and a listener whose accept queue
was full (so the loopback `connect` blocks):

```text
connect(3, {... sin_port=htons(9000), sin_addr=inet_addr("127.0.0.1")}, 16) = ? ERESTARTSYS
--- SIGUSR1 {si_signo=SIGUSR1, si_code=SI_TKILL, ...} ---
[ERROR] nros: [    0.010000] zpico Session -> ConnectionFailed
```

That is the nightly's line, byte for byte. A loopback `connect` normally
completes inside the syscall, so the window is only open on a loaded host —
the nightly started nine threadx-linux cells at once.

## Fix

`packages/drivers/net/nsos-netx/src/nsos_netx.c`: every call that can block
(`connect`, `accept`, `send`, `sendto`, `recv`, `recvfrom`, `select`) finishes
what it started instead of returning EINTR. An interrupted `connect` keeps
going in the kernel, so the shim waits for it (`poll` POLLOUT) and reads
`SO_ERROR`; a re-issued `connect` would only report EALREADY. The others are
re-issued, and each waits with `poll` against ONE deadline taken when the call
began, from the socket's own `SO_RCVTIMEO`/`SO_SNDTIMEO`, so a socket timeout
keeps its meaning however often the thread is preempted. This is not a retry
of a failed operation: nothing about the socket changed.

Sibling not covered by this fix: the FreeRTOS POSIX port
(`portable/ThirdParty/GCC/Posix/port.c`) installs its SIGALRM tick handler
with `sa_flags = 0` too, so a host call a `freertos-posix` task blocks in can
see EINTR the same way. Its network path is not `nsos-netx`, and its zenoh
image does not run yet (issue 2341); check it when that board gets a
networked cell.

The quiet-link cell now also prints the router's state (alive, accepting, its
log) when the image fails to come up, and the nightly uploads
`test-logs/fixtures/` per platform, so the router's half of a red like this is
on record next time.
