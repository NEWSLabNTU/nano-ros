---
id: 1802
title: "`declares_keep_running_across_a_reconnect` SIGSEGVed once in a `just ci gate` sweep with issue 1751's fix in the pin; solo it passes 3 of 3"
status: open
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

## Next

Reproduce under load, with a core or under gdb: run the test in a loop beside
a CPU hog, or with `--test-threads` matching the sweep. Then name the
faulting frame. 1751's table measured 6 of 6 passes under gdb, solo, which
does not cover the loaded case.
