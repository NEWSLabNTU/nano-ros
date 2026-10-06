---
id: 1711
title: "A native multi-tier C++ entry SEGVs intermittently during tier setup —
  heap corruption, caught in `malloc` under `z_declare_publisher` on a tier
  thread (`derived-tiers-cpp` `native_entry`)"
status: open
type: bug
area: [runtime, cpp, zenoh, tiers]
severity: medium
found: 2026-10-06
related: [1693, 1535, 1575]
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
