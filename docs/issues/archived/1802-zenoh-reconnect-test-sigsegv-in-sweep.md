---
id: 1802
title: "`declares_keep_running_across_a_reconnect` SIGSEGVed once in a `just ci gate` sweep with issue 1751's fix in the pin; solo it passes 3 of 3"
status: resolved
type: bug
area: [runtime, zenoh, testing]
severity: medium
found: 2026-10-11
related: [1751, 1749]
---

## Measured

`just ci gate`, step `test-unit`, on branch `work/phase-487-w2` rebased onto
`c948538252`. That branch changes nothing under `packages/rmw`. The zenoh-pico
pin is `199e611fd5`, which contains `eb16447f` ("a DECLARE_FINAL holds the
interest it calls back into"), the issue 1751 fix.

```
SLOW [> 45.000s] nros-rmw-zenoh::zenoh_integration declares_keep_running_across_a_reconnect
SIGSEGV [  50.470s] nros-rmw-zenoh::zenoh_integration declares_keep_running_across_a_reconnect
Summary [  50.841s] 1947 tests run: 1946 passed, 1 failed, 6 skipped
```

The test then passed 3 runs of 3 when run on its own:
`cargo nextest run -p nros-rmw-zenoh --features platform-posix --test zenoh_integration declares_keep_running_across_a_reconnect`,
40.2 s each. A second full `just test-unit` on the same tree passed 1947 of
1947.

That is 1 failure in 5 runs, and the only failure was under sweep load. The
failing run also took 50 s against 40 s solo. The sweep run captured no core
and no backtrace, so this record cannot say whether the crash is 1751's frame
(`_z_interest_process_declare_final`) or a different one.

## Cause: the test, not zenoh-pico

The SIGSEGV is a use-after-free in the test itself, and it only happens once the test
has already failed.

The four declaring threads reach the session through a raw pointer to `ctx`,
which lives on the test thread's stack. On the success path the test sets
`stop` and joins them before `ctx` drops. On every other path it did not. A
`wait_for` timeout panics, the test thread unwinds and exits, and the workers
keep calling `declare_*` through a pointer into a dead stack frame.

The 50.47 s of the original report is the 50 s resume timeout firing under
load, and the crash comes right after it.

## Reproduced (2026-10-12)

Each test ran in its own process, as nextest runs it, 8 or 16 at a time,
with core dumps on.

- **Under load:** 96 runs at the real 50 s limit, beside 32 CPU hogs. 95
  passed. One timed out (`timed out after 50s waiting for declares to resume
  after the restart`), and that run ended as a panic, not a crash.
- **Inside the window:** with the resume limit temporarily set to 39 s, so the
  panic lands while `_z_reopen` replays, `main`'s test crashed. The core's
  faulting thread is a worker:

  ```
  #0 zpico_claim_slot_impl (… s=0x5ea49aa0f43f) at zpico.c:2552
  #1 zpico_declare_liveliness (session=0x5ea49aa0f43f, keyexpr="issue1749/t3/2")
  #4 nros_rmw_zenoh::zpico::Context::declare_liveliness
  #5 zenoh_integration::declares_keep_running_across_a_reconnect::{closure}
  ```

  `session` is misaligned garbage, read through the dead frame. It is not
  1751's `_z_interest_process_declare_final`.
- **Rate at the 39 s limit:** `main`'s test SIGSEGVed in 11 of 192 runs
  (1 of 16 and 0 of 48 at 8 at a time, 10 of 128 at 16 at a time). The
  fixed test SIGSEGVed in 0 of 176 (0 of 48, then 0 of 128 at 16 at a time).
  Every one of its runs ended as the clean timeout panic, `rc=101`.
- **Not reproducible in gdb:** under `gdb` the window never opened (48 of 48
  passed beside 24 CPU hogs). That is why the first attempts found nothing.

## Fix

- The workers are held in a `StopAndJoin` guard, declared after `ctx` so it
  drops first. On the success path `finish()` stops and joins them, and still
  reports a worker's panic. On a panic, `Drop` stops and joins them before the
  unwind reaches `ctx`. A timeout now reports as the timeout it is.
- The resume wait is 75 s, up from 50 s. It normally resumes at 39–40 s: the
  window runs measured 16 of 16 at a 40 s limit and 15 of 16 at 41 s. Under
  load it took over 50 s in 1 run of 96.
- The nextest override grows from 2 to 3 periods (135 s) to cover the worst
  case, 10 + 25 + 75 s.

Sweep: the only other test that shares a context with threads through a raw
pointer is `concurrent_declares_on_one_session_never_share_a_slot`. It joins
every worker before it asserts anything, and a dead worker there leaves the
others blocked on a barrier, not touching the session.

## Also seen, not this issue

Running the whole `zenoh_integration` binary under libtest's own threads
opens several sessions in one process at once. That crashed in `z_open`'s
config handling on `g_sessions[0]` (`_z_hashmap_insert`; `double free` in
`z_config_drop`). nextest never runs these tests that way, and the binary
fails on `open the session: Full` long before then, so it is recorded here
and nothing else.
