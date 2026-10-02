---
id: 1605
title: "`rmw-alloc-sites.py` reports no XRCE row at all: its call list predates
  the `nros_xrce_calloc` funnel helper, so seven real allocation sites are invisible"
status: resolved
type: tooling
area: tooling, xrce
severity: low
found: 2026-10-01
related: [0777, 0816, 0832]
resolved_in: "branch fix/build-correctness-1593-1596-1599-1605"
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

## Resolution

Fixed the class — helpers are FOLLOWED, not listed — plus a second blind spot
the fix exposed.

- **Helpers.** `BASE_ALLOCATORS` now includes the platform ABI
  (`nros_platform_alloc`, `nros_platform_realloc`). `find_helpers()` computes, to
  a fixed point, every function that reaches an allocator (or another helper)
  AND is either `inline` or defined in a HEADER — the two shapes whose body is
  never scanned as a site. Each CALL of a helper is a site attributed to the
  caller; the helper's own body is not (no double count). An out-of-line helper
  in a `.c` cannot be told apart from a constructor by shape, so it is declared
  in `DECLARED_HELPERS` with a reason (`z_malloc`/`z_realloc`, zenoh-pico's hooks).
- **A row for every backend scanned**, including `0 / 0`, and the report prints
  which helpers it followed.
- **Second defect, found by the first:** `HEAD` required a return-type prefix
  before the name, so a definition clang-format wraps as
  `rmw_ret_t\nxrce_subscription_create(` never matched, and its body was
  attributed to the PREVIOUS function — `subscriber.c:114` first came out as
  "in `xrce_topic_callback()`", a per-message callback. The prefix is optional
  now. Had that site been a steady-state one, `--check` would have classed it
  wrongly in either direction.
- **Self-tests (7 cases, run on every invocation):** a header `static inline`
  funnel is followed and its call counted; a chained inline helper is followed;
  an out-of-line constructor is NOT a helper; a wrapped-signature definition
  names itself (observed failing with the old `HEAD` before the fix).

Measured (`python3 scripts/rmw-alloc-sites.py --check`), before → after:

| backend | before | after |
| --- | --- | --- |
| cyclonedds | 1 / 7 | 1 / 8 (`session.cpp:83` `nros_platform_alloc` in `alloc_session_state`, newly visible) |
| uorb | 0 / 3 | 0 / 3 |
| xrce | (no row) | 0 / 7 — exactly the seven sites listed above |
| zenoh | (no row) | 0 / 0 |

`--check` stays OK: no new steady-state site. `book/src/design/rmw.md`'s
snapshot table (which still read 6/6 and 0/9) is updated to these numbers.

Sweep: `git grep -nE 'static inline[^;]*\*\s*\w+\s*\(' -- 'packages/rmw/**/*.h'`
— `nros_xrce_calloc` was the only allocating inline helper in a header.

Not addressed: `graph_query.cpp:100-101` still attribute to `<file scope>`
(they are in an INDENTED class method, `EndpointBatch::read`, which column-0
HEAD cannot see). They classify as create/init, which is correct for a graph
query, so the attribution is cosmetic here.
