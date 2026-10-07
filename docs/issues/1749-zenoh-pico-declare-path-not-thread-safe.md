---
id: 1749
title: "zenoh-pico's declaration cache is unlocked: concurrent declares on one session tore it (fixed in the shim), and the lease task's reconnect replay still walks it unlocked"
status: open
type: bug
severity: low
area: [runtime, zenoh, tiers]
related: [1733, 1711, 0447]
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

## What remains open

**Reconnect replay.** With `Z_FEATURE_AUTO_RECONNECT`, the LEASE task's
`_z_reopen` (`src/net/session.c:236`, called from `transport/*/lease.c`)
iterates `_declaration_cache` to replay it (`session.c:283`). It does so on its
own thread, not under the shim's lock, so a declare that coincides with a
reconnect still races that walk. It is narrower than what was fixed, because it
needs a link drop during a declare. It cannot be closed from the shim: the
lease task is zenoh-pico's. The fix belongs on the zenoh-pico fork's patch
line: take the session mutex in `_z_cache_declaration`,
`_z_prune_declaration` and the replay walk, and make the entity and resource
id counters atomic or locked. Nothing measured here reached it.

Acceptance for closing: an upstream-side lock on `_declaration_cache`, and a
test that declares in a loop across a forced reconnect.
