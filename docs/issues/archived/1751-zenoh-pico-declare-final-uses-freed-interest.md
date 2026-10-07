---
id: 1751
title: "zenoh-pico's read task called a write filter back through a freed interest: `_z_interest_process_declare_final` kept a pointer into the interest list past its lock (SIGSEGV, `ctx=0x0`)"
status: resolved
type: bug
severity: medium
area: [runtime, zenoh]
related: [1749, 1733, 1711]
found: 2026-10-08
---

## Measured

Found while writing issue 1749's regression test,
`zenoh_integration.rs::declares_keep_running_across_a_reconnect`: four threads
declare and drop publishers and liveliness tokens on one session while the
router is killed and restarted on the same port. With 1749's cache lock in
place, the test still died, 2 runs of 2, on the READ task:

```
Thread 3 received signal SIGSEGV
#0  ___pthread_mutex_lock (mutex=0x10)
#3  _z_write_filter_mutex_lock (ctx=0x0)              (src/net/filtering.c:49)
#4  _z_write_filter_callback (msg=..., peer=..., arg=0x0)  (filtering.c:197)
#5  _z_interest_process_declare_final (zn=..., id=3683, peer=...)  (src/session/interest.c:519)
#6  _z_handle_declare_inner                          (src/session/rx.c:78)
    ... _zp_unicast_read_task
```

`_z_interest_process_declare_final` looked the interest up under
`_mutex_inner` and kept the POINTER `__unsafe_z_get_interest_by_id` returns --
the list node's own rc -- after releasing the lock. A publisher's undeclare on
another thread (`_z_write_filter_clear` -> `_z_remove_interest` ->
`_z_unregister_interest`) frees that node, so the callback ran through freed
memory. The reconnect makes it likely, not possible: the replay re-sends every
cached interest and the router answers each with a DECLARE_FINAL while the
publishers that own them come and go. ThreadSanitizer saw the same pair
(`free` / `_z_interest_process_declare_final`, `_z_session_interest_rc_drop` /
`_z_interest_process_declare_final`) in every run of 1749's C harness, before
and after the cache lock. Upstream zenoh-pico `main` has the same code.

## Fixed

On the zenoh-pico fork's `nano-ros` patch line, as its own commit after 1749's
("a DECLARE_FINAL holds the interest it calls back into"): the lookup clones
the rc under the lock and drops it after the callback, which is what
`__z_get_interest_by_key_and_flags` already does for declares.

| check | before | after |
| --- | --- | --- |
| `declares_keep_running_across_a_reconnect` under gdb | 2 / 2 SIGSEGV | 6 / 6 pass |

TSan still prints three reports per 40 s run of 1749's C harness that name
`_z_interest_process_declare_final`, and they are a different shape: the read
task now holds a CLONE, drops it with a release `fetch_sub`, and the
unregistering thread's last drop frees after the acquire fence in
`_z_rc_decrease_weak`. That is the ordinary refcount protocol, which is
correct; ThreadSanitizer does not model `atomic_thread_fence`, so it reports
every such free. Before the change the reports were reads of the interest
through the stale list pointer with no reference held at all.
