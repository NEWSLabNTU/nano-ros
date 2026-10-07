---
id: 1749
title: "zenoh-pico's declaration cache was unlocked: concurrent declares on one session tore it (fixed in the shim), and the lease task's reconnect replay walked it unlocked (fixed in the zenoh-pico fork)"
status: resolved
type: bug
severity: low
area: [runtime, zenoh, tiers]
related: [1733, 1711, 1751, 0447, 0899, 0924]
found: 2026-10-07
---

## Measured

Issue 1733 ran `derived-tiers-cpp`'s native image under
`valgrind --tool=helgrind` with its pool short-fall fixed, so that all four
derived tiers set up and three of them did so concurrently on the shared
session. Helgrind reported 14 races on zenoh-pico's declare path inside tier
setup, in two places:

```
Possible data race during write of size 4 by thread #5
   _z_get_entity_id            (zenoh-pico/src/session/resource.c:56)
   _z_declare_publisher        (src/net/primitives.c:143)
   z_declare_publisher         (src/api/api.c:1448)
   zpico_declare_publisher_ex  (zpico-sys/c/zpico/zpico.c:2557)
   ... nros_cpp::native_tier_trampoline
This conflicts with a previous write of size 4 by thread #4
   _z_get_entity_id            (resource.c:56)
   _z_add_interest             (primitives.c:645)
   _z_write_filter_create      (filtering.c:289)
   z_declare_publisher         (api.c:1450)

Possible data race during write of size 8 by thread #5
   _z_slist_push_back          (src/collections/list.c:317)
   _z_network_message_slist_push_back
   _z_cache_declaration        (src/net/session.c:305)
   _z_send_declare             (src/net/primitives.c:58)
   ... z_declare_publisher ... tier 2's setup
This conflicts with a previous write of size 8 by thread #4
   _z_cache_declaration        (session.c:305)
   _z_send_declare
   _z_liveliness_send_declare_token ... tier 3's setup
```

Both are plain unsynchronized writes in upstream zenoh-pico:

* `_z_get_entity_id` is `return zn->_entity_id++;`. Two concurrent declares
  can take the SAME entity id.
* `_z_cache_declaration` (compiled under `Z_FEATURE_AUTO_RECONNECT`) pushes the
  declare onto `zs->_declaration_cache`, a linked list, with no session lock.
  Two concurrent pushes tear the list. That is heap corruption, and it is the
  same allocation site (`_z_slist_new*` under `z_declare_*`) where issue 1733's
  one gdb-caught crash sat.

### It already crashes a unit test on `main`

Issue 1711's own regression test,
`nros-rmw-zenoh/tests/zenoh_integration.rs::concurrent_declares_on_one_session_never_share_a_slot`,
runs four threads that declare and drop two publishers each on one session.
It SIGSEGVs intermittently. Measured on this checkout, running the test alone
in a loop (`cargo nextest run -p nros-rmw-zenoh --features platform-posix`):

| tree | runs | SIGSEGV |
| --- | --- | --- |
| `origin/main` code (every crate the test reaches) | 100 | 3 |
| issue 1733's branch | 100 | 4 |

It also failed once in a `just ci gate` `test-unit` step. 1711 reported "5 of
5" passing, which is consistent with a 3-4 % rate. Caught under gdb, three
threads are inside the UNDECLARE half of the same list at once, and the one
that faults follows a garbage node pointer (`right=0x7ff813fed079`):

```
Thread 9 received signal SIGSEGV
#0  _z_cache_declaration_undeclare_filter_interest (left=..., right=0x7ff813fed079)  (src/net/session.c:321)
#1  _z_slist_drop_filter
#2  _z_network_message_slist_drop_first_filter
#3  _z_prune_declaration
#4  _z_remove_interest
#5  _z_write_filter_clear
#6  _z_undeclare_publisher
#7  z_undeclare_publisher
#8  zpico_undeclare_publisher
#9  drop_glue<nros_rmw_zenoh::zpico::Publisher>   (zenoh_integration.rs:1438)
  10  Thread ... _z_cache_declaration_undeclare_filter_kexpr (..., right=0x7ff813fed079)
  11  Thread ... _z_cache_declaration_undeclare_filter_interest (..., right=0x7ff813fed079)
```

So `_z_prune_declaration` (the undeclare side) is as unguarded as
`_z_cache_declaration` (the declare side). The suite carries this as a
3-4 % flake in `test-unit`, which the merge queue runs.

## Fixed in the shim (2026-10-07, with issue 1733)

**Tier setup.** Issue 1733 serializes every tier runner's setups. The RTOS
runners already chained their spawns (issue #144), and the Linux Rust runner
held a lock (issue 0447). The native C++ and NuttX Rust runners now hold the
one shared `TierSetupGate`.

**Every declare, below every runner.** `zpico.c` holds a per-session RECURSIVE
`declare_mutex` (`zpico_declare_lock` / `zpico_declare_unlock`) across each
call into zenoh-pico's declare path:

* `z_declare_*` / `z_undeclare_*` for publishers, all five subscriber variants
  and queryables;
* the liveliness token and the graph-cache liveliness subscriber;
* the three `z_get` sites and both `z_liveliness_get` sites, which take an
  entity id;
* the close-path undeclares.

The lock is recursive because a declare can deliver synchronously to a local
subscriber or queryable whose callback is ours. On ordering, read rather than
assumed:

* the lock comes first and zenoh-pico's own mutexes inside it;
* no callback in `zpico.c` declares, so no thread holding a zenoh-pico mutex
  ever waits on this lock;
* the subscription path releases the session mutex before it runs callbacks
  (`subscription.c`);
* every `_z_cache_declaration` / `_z_prune_declaration` caller is an API call:
  `_z_send_declare` / `_z_send_undeclare`, `_z_add_interest` /
  `_z_remove_interest`, and the write filter's create and clear, which run
  inside `z_declare_publisher` / `z_undeclare_publisher`.

Measured:

| check | before | after |
| --- | --- | --- |
| 1711's `concurrent_declares_on_one_session_never_share_a_slot`, run alone | 3 / 100 SIGSEGV (`origin/main`) | 0 / 500 |
| helgrind, `derived-tiers-cpp` native, races on `_z_cache_declaration` / `_z_prune_declaration` / `_z_get_entity_id` | 10 (no gate) / 6 (gate only) | 0 |
| `just ci gate` `test-unit` | red 2 of 3 runs, on this test | green |


## The reconnect replay (fixed in zenoh-pico, 2026-10-08)

With `Z_FEATURE_AUTO_RECONNECT`, the LEASE task's `_z_reopen`
(`src/net/session.c`, reached from `transport/*/lease.c`) walks
`_declaration_cache` to replay it after a reconnect. It did so on its own
thread and under no lock, so the shim's lock above could not reach it.

### Reproduced

A host harness (zenoh-pico built standalone with the fork's sources, lease
lowered to 1 s, ThreadSanitizer) opens one client session to a private
`rmw_zenohd`, runs four threads that declare and undeclare publishers,
subscribers and liveliness tokens in a loop, and kills and restarts the router
every 2-3 s by PID (13 reconnects per 40 s run). The application's declares
are serialized by one mutex, as the shim's lock does, so the only unsynchronized
party left is the lease task. Reports whose stacks reach the cache, the entity
id or the replay's encode:

| tree | run 1 | run 2 | run 3 | declares NOT serialized |
| --- | --- | --- | --- | --- |
| fork pin before (`e28ff603`) | 37 | 8 | 23 | 208 |
| with the fix | 0 | 0 | 0 | 0 |

The before runs include the use-after-free the issue predicted: the replay
encoding a node an undeclare had just freed.

```
heap-use-after-free  read by T1 (the lease task)
  _z_declare_encode         (src/protocol/codec/network.c:390)
  _z_send_n_msg             (src/transport/common/tx.c:664)
  _z_reopen                 (src/net/session.c:287)
  _zp_unicast_failed        (src/transport/unicast/lease.c:196)
freed by T4
  _z_slist_drop_filter      (src/collections/list.c:394)
  _z_prune_declaration      (src/net/session.c:334)
  _z_send_undeclare ... z_liveliness_undeclare_token
```

Under AddressSanitizer the same harness ran 60 s each way without a fault: the
window is a few microseconds per reconnect, so the measured evidence is the
race detector's, not a crash.

### Fixed

Two commits on the fork's `nano-ros` patch line (`jerry73204/zenoh-pico`),
each its own patch:

1. **"lock the declaration cache against the reconnect replay".**
   * `_declaration_cache` is guarded by the session's existing recursive
     `_mutex_transport`, taken in `_z_cache_declaration`,
     `_z_prune_declaration` and across `_z_reopen`'s walk. No new mutex: every
     caller of the two cache functions has just returned from `_z_send_n_msg`
     in the same frame, and `_z_send_n_msg` takes `_mutex_transport` (issue
     0899), so taking it again adds no lock-order edge that did not exist. The
     replay's own sends take it recursively.
   * A replay send that fails ends the replay and returns the error, instead of
     retrying the same message forever. Holding the lock through that retry
     would have hung every declare and every publisher behind a dead link. The
     lease task the reopen just started fails over in its turn (issue 0924's
     claim is released first) and replays from the top.
   * `_entity_id` is a `_z_atomic_size_t` with a relaxed fetch-add, truncated
     to the 32-bit wire id, so it wraps as `uint32_t++` did. A lock would not
     do: `_z_get_entity_id` is reached from paths that already hold
     `_mutex_inner` or `_mutex_transport`. `_resource_id` was already under
     `_mutex_inner` and is only commented.
2. **"a DECLARE_FINAL holds the interest it calls back into"** — issue 1751,
   which the regression test below found.

**Lock order.** The shim's per-session declare lock, then `_mutex_transport`.
The replay runs on the lease task and never takes the shim's lock, and no
zenoh-pico callback declares, so the reverse order cannot occur.

**The shim's lock stays.** It is one uncontended take per declare, and it
still serializes the rest of zenoh-pico's declare path (registration lists,
write filters, interests) against concurrent tier setups, which has not been
audited lock by lock. Its comment in `zpico.c` now says so.

### Verified

| check | result |
| --- | --- |
| harness above, TSan, cache / entity-id / replay reports | 37, 8, 23, 208 -> 0, 0, 0, 0 |
| `declares_keep_running_across_a_reconnect` (new), under gdb | 6 / 6 pass, resumed 39.8 s after the restart each run |
| 1711's `concurrent_declares_on_one_session_never_share_a_slot`, alone | 0 / 500 failed |
| 1733's boot harness, `derived-tiers-cpp` `native`, 6 s | 24 runs: 24 ok, 0 lost, 0 crash |
| 1733's boot harness, `realtime-rust` `native_derived`, 30 s | 24 runs: 24 ok (both tiers publishing, 0 setup failures, 0 crashes) |

### Regression test

`nros-rmw-zenoh/tests/zenoh_integration.rs::declares_keep_running_across_a_reconnect`
declares and drops publishers and liveliness tokens on four threads while the
router is killed and restarted on the same port. It requires the outage to be
seen (a declare fails) and the session to come back (50 more declare rounds
succeed after the restart), so it cannot pass without the replay having run
under the churn. It takes ~40 s at the default 10 s lease and has a nextest
override for that. Without a race detector it cannot see the cache race itself,
which needs microsecond timing; it did catch issue 1751 every time.

### Still in the harness's TSan output, not this issue

* The lease task's `_received` / `_transmitted` flags and `_zp_unicast_failed`'s
  reads of `_lease_task_running` are plain `bool`s shared with the read task.
* A lease task replaced by a reconnect is never joined (zenoh-pico's own
  `TODO: join tasks`), which TSan reports as a thread leak.
