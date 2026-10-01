---
id: 1605
title: "`rmw-alloc-sites.py` reports no XRCE row at all: its call list predates
  the `nros_xrce_calloc` funnel helper, so seven real allocation sites are invisible"
status: open
type: tooling
area: tooling, xrce
severity: low
found: 2026-10-01
related: [0777, 0816, 0832]
---

## Problem

`book/src/design/rmw.md` sends readers to `scripts/rmw-alloc-sites.py` for
"your backend's row" when planning a heap budget. On 2026-10-01 the report
lists only `cyclonedds` (1 steady-state, 7 create/init) and `uorb` (0 / 3).
There is **no `xrce` row**, which reads as "XRCE does not allocate".

It does. Issue 0832 routed every XRCE allocation through
`nros_xrce_calloc` / `nros_xrce_free` (`packages/rmw/xrce/nros-rmw-xrce/src/internal.h`)
so they reach `nros_platform_alloc`, and the report's call regex
(`ALLOC`, `scripts/rmw-alloc-sites.py:43`) names only the libc/ddsrt spellings:

```
$ git grep -n "nros_xrce_calloc(" -- packages/rmw/xrce/nros-rmw-xrce/src | grep -v "static inline"
publisher.c:54  service.c:137  service.c:596  session.c:483
subscriber.c:114  transport_nros_udp.c:96  transport_nros_udp.c:97
```

All seven are open/create paths (so "create/init" in the report's own terms),
none per message.

Found while fixing issue 0816: three book sentences said XRCE needs "no heap"
and the report could not contradict them.

## Shape of the fix

Add the funnel helpers to the call set (`nros_xrce_calloc`, and generally any
`static inline` wrapper whose body reaches `nros_platform_alloc`), and make the
report print a row for EVERY backend directory it scanned, including a `0 / 0`
one — an absent row and a zero row must not look the same. A selftest that a
`nros_xrce_calloc(` site is counted.
