---
id: 1711
title: "A native multi-tier C++ entry SEGVs intermittently during tier setup —
  heap corruption, caught in `malloc` under `z_declare_publisher` on a tier
  thread (`derived-tiers-cpp` `native_entry`)"
status: resolved
type: bug
area: [runtime, cpp, zenoh, tiers]
severity: medium
found: 2026-10-06
related: [1693, 1535, 1575, 1734]
---

## What was measured

`examples/workspaces/derived-tiers-cpp`, `nros build demo_bringup:native`
(four C++ components, four DERIVED tiers over one zenoh session), booted
against a private `rmw_zenohd` with `NROS_ENTRY_SPIN_MS=2000`:

| tree | runs | SIGSEGV |
| --- | --- | --- |
| `origin/main` 52dcf5b5a2 (before issue 1693's fix) | 4 | 1 (exit 139) |
| issue 1693's branch | 1 plain + 14 under gdb | 1 + 1 |

So it predates issue 1693's change, and is not caused by it: that change
leaves a normal boot's path identical (it only arms nothing in a census run,
and makes a FAILED reporter non-fatal).

The backtrace (gdb, `thread apply all bt`):

```
Thread 4 "native_entry" received signal SIGSEGV
#0  tcache_get                       malloc/malloc.c:3196
#1  __GI___libc_malloc (bytes=16)
#2  nros_platform_alloc (size=16)    nros-platform-posix/src/platform.c:83
#3  z_malloc                         zpico-sys/c/zpico/platform_aliases.c:41
#4  _z_rc_init                       zenoh-pico/src/collections/refcount.c:32
#7  _z_keyexpr_declare_prefix        zenoh-pico/src/session/keyexpr.c:522
#8  _z_declare_publisher             zenoh-pico/src/net/primitives.c:152
#11 zpico_declare_publisher_ex (keyexpr="0/system/stop_mode/control/std_msgs::msg::dds_::Int32_/TypeHashNotSupported")
#19 nros_cpp::publisher::nros_cpp_publisher_create
#22 stop_mode_pkg::StopModeOperator::StopModeOperator(nros::NodeHandle)
#23 __nros_entry_setup_tier_1(void*)
#24 nros_cpp::native_tier_trampoline   nros-cpp/src/lib.rs:5788
```

The other threads at the crash: zenoh-pico's read task in `recv`, its lease
task in `nanosleep`, and the main thread in `NodeWake::wait_ms`. A crash
inside `tcache_get` is the heap's free-list already being corrupt; the
corrupting write happened earlier and is not on this stack.

## Not measured

Where the corrupting write is. Candidates the shape suggests, none tested: a
tier thread declaring on the shared zenoh session concurrently with another
tier's setup or with the read task (the session is shared across tiers by
design, RFC-0015 Model 1); and the tier executors' storage
(`__nros_tier_executor_storage`) being smaller than an executor's real
footprint for one of the tiers. A run under ASan/valgrind would answer it.

## Reproduce

Start `rmw_zenohd` on a private port
(`ZENOH_CONFIG_OVERRIDE='listen/endpoints=["tcp/127.0.0.1:17693"]'`), then
`NROS_LOCATOR=tcp/127.0.0.1:17693 NROS_ENTRY_SPIN_MS=2000 ./build/posix-zenoh-native/cmake/native_entry`
in a loop; roughly one run in four to fifteen dies with 139.

## Resolution (2026-10-07)

The corrupting write was a race in OUR zenoh shim, not in zenoh-pico.

**Found with ASan.** The native entry was rebuilt with
`-fsanitize=address` in a scratch build dir, over the same generated
CMakeLists. 16 of 20 boots reported a heap-use-after-free:

- the memory was allocated by tier thread T5, in its
  `z_declare_publisher` → `_z_keyexpr_declare_prefix`;
- it was freed by tier thread T4, in ITS `z_declare_publisher`'s failure path
  (`_z_undeclare_publisher`);
- it was then read by T5 while registering the resource.

Two threads were writing into one publisher object.

**Cause.** `zpico_declare_publisher_ex` found the first slot with `!active`,
declared into `s->publishers[idx]`, and set `active = true` only afterwards.
The session is shared by every tier thread (RFC-0015 Model 1), and each tier's
setup declares its entities at boot. Two concurrent declares therefore took
the same slot. The same check-then-act shape was in all eight claim sites:
publishers, the five subscriber variants, liveliness and queryables.

**Fix.** `zpico_claim_slot` / `zpico_release_slot`
(`zpico-sys/c/zpico/zpico.c`) find and MARK a slot under a new session
`slot_mutex`, so the slot is the caller's before its declare starts. The
declare itself runs outside the lock, because a callback on the read task may
declare, and zenoh-pico takes its own session mutex inside `z_declare_*`.

- Every failure return between the claim and the declare's success gives the
  slot back.
- The four undeclare paths release through the same lock.
- Without `Z_FEATURE_MULTI_THREAD` there is no lock.

**Measured on the issue's reproduction** (`derived-tiers-cpp`
`native_entry`, private `rmw_zenohd`, `NROS_ENTRY_SPIN_MS=2000`):

| build | before | after |
| --- | --- | --- |
| plain | 5 / 20 SIGSEGV | 0 / 40 |
| ASan | 16 / 20 use-after-free | 0 / 40 |

**Regression test.**
`nros-rmw-zenoh/tests/zenoh_integration.rs::concurrent_declares_on_one_session_never_share_a_slot`
runs four threads × two publishers on one session, released together by a
barrier, for 20 rounds, and asserts every handle is distinct.

- Against the old `zpico.c` it dies with SIGSEGV 3 of 3.
- With the fix it passes 5 of 5, in 0.16 s.

**Sibling filed.** The XRCE shim has the same unguarded check-then-act in four
slot tables, on top of a client library with no thread-safety at all. That is
issue 1734, a separate fix because it needs a session-level lock or a stated
single-thread rule, not just a claim lock.
