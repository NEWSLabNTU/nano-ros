---
id: 1056
title: "`assert_no_session_churn` can PASS on a broken lease — but not because of
  start skew, and lengthening the window does not fix it"
status: resolved
type: bug
area: testing
severity: medium
found: 2026-09-04
resolved: 2026-09-05
related: [issue-1044, issue-1013, issue-0906, phase-424]
---

## The hole as filed

`MAX_ROUTER_SESSIONS = 3` stands in for a rate with a count. A client lease
`L < 30 s` cannot hear the router's 30 s keep-alive, so each node re-dials every
`2L`; in a 60 s window that is one re-open per node, two nodes, four sessions —
over the limit. That holds only if BOTH nodes lapse inside the window, and they
do not start together, so the later node's only lapse could land past the
window's end and leave 3 sessions and a PASS on a broken build.

**Direction proposed:** lengthen the window to ~120 s, so every lease under 30 s
produces at least two re-opens per node. Cost: roughly doubles the pub/sub cell.

## RESOLVED 2026-09-05 — the hole is real, the mechanism is not, and the fix
## does not deliver

The arithmetic was re-derived from `_zp_unicast_lease_task`
(zenoh-pico `src/transport/unicast/lease.c:219-279`) and checked against every
measurement available on this host. Three of the four load-bearing claims are
wrong, and the fourth — that a broken build can pass — is right for a reason that
no affordable window closes.

### 1. One node lapses, not two

The lease task wakes every `lease` ms and closes only if `_received` was false
for that whole window; ANY inbound frame sets it. The LISTENER is fed a 1 Hz
sample stream by the router, so its check windows always contain traffic and it
never lapses. The TALKER hears nothing back but keep-alives. Our own fork's
`config.h` says it in one line: *"a pure publisher hears nothing back and closes
at 2 x Z_TRANSPORT_LEASE"*.

So `sessions = 2 + talker lapses`, not `2 + lapses(A) + lapses(B)`.

**The evidence was already in the doc comment and had been noticed without being
chased.** The per-node table predicted ~8 sessions for issue 0906's 10 s lease
and the cell measured **5**; the discrepancy was written down as
`~8 (5 measured)`. One node gives `2 + floor(60 / 20) = 5` exactly.

### 2. The lapse period is `2L` only while `2L < 30 s`

The check window is `L` wide; `rmw_zenohd` speaks every 30 s. The session dies at
the end of the first `L`-window that no keep-alive lands in — a beat between `L`
and 30, which runs away as `L -> 30`. Simulated from the loop above:

| lease | first close | ratio to L |
| ---: | ---: | ---: |
| 10 s | 20.0 s | 2 |
| 14 s | 28.0 s | 2 |
| 15 s | 45.0 s | 3 |
| 18 s | 54.0 s | 3 |
| 20 s | 80.0 s | 4 |
| 24 s | 144.0 s | 6 |
| 29 s | **899.0 s** | 31 |
| 29.9 s | **9000 s** | 301 |
| >= 30 s | never | — |

So the acceptance as filed — *"a build with `Z_TRANSPORT_LEASE_MS` anywhere in
`(0, 30_000)` fails this cell regardless of how far apart the two nodes start"* —
**is not reachable by any affordable window**. Every `L < 30 s` does close
eventually, so the acceptance is not impossible in principle — it is unpayable:
covering `L = 29.9 s` needs two lapses at 9000 s each, five hours per cell,
twelve cells.

### 3. Start skew is not the exposure

Both nodes must participate for all `PUBSUB_MIN_SAMPLES` samples — the talker
publishes all 60, the listener hears all 60 — so each one's observed session life
is at least that span WHATEVER the skew. Skew lengthens the EARLIER node's life
and leaves the later one alone.

Measured across the nine `test-logs/fixtures/zenohd-*.log` router logs on this
host (2026-09-04):

| log | router span | session opens, relative | sessions |
| --- | ---: | --- | ---: |
| 7800 | 82.0 s | 2.05 s, 22.04 s | 2 |
| 7900 | 82.2 s | 2.10 s, 21.99 s | 2 |
| 8000 | 82.1 s | 2.11 s, 22.14 s | 2 |
| 8200 | 65.5 s | 5.28 s, 5.28 s | 2 |
| 8300 | 60.6 s | 0.07 s, 0.07 s | 2 |
| 8400 | 60.6 s | 0.06 s, 0.06 s | 2 |
| 9000 | 61.4 s | 0.00 s, 1.00 s | 2 |
| 9100 | 61.5 s | 0.00 s, 1.00 s | 2 |
| 9200 | 61.5 s | 0.00 s, 1.00 s | 2 |

Skew ranges 0.00 s to 20.0 s, and the later node is alive ~60 s in every one
(82.0 - 22.04 = 60.0). The span grows with the skew; the later node's life does
not shrink. Exactly 2 sessions in 9 of 9, on the shipped 60 s lease.

### 4. What the cell DOES cover, and what a longer window would buy

Failing needs 2 lapses (the slack is one), all from the talker, whose life is
~60 s. So the covered band is `2 x first_close(L) <= 60`, i.e. **`L <= 14 s`**.

| talker life | added cell time | band covered |
| ---: | ---: | --- |
| 60 s (today) | — | `L <= 14 s` |
| 90 s | +30 s x 12 cells = +6 min | `L <= 15 s` |
| 120 s (as filed) | +60 s x 12 = +12 min | `L <= 18 s` |
| 180 s | +120 s x 12 = +24 min | `L <= 22 s` |
| 300 s | +240 s x 12 = +48 min | `L <= 24 s` |

Doubling the cell moves the frontier by four seconds of lease. What is actually
shipped is 60 s (ours) and **10 s** — zenoh-pico's own upstream default
(`CMakeLists.txt:245`, `config.h:53`), which is both issue 0906's value and what
a regression here would revert to. That is inside the covered band with margin:
5 sessions against a limit of 3.

### Decision: the window is NOT lengthened

Paying 12 minutes of suite time to move the frontier from 14 s to 18 s, for lease
values nobody has ever shipped, is a bad trade — and the benefit the issue priced
it against (covering all of `(0, 30_000)`) does not exist.

**What landed instead**, in `packages/testing/nros-tests/tests/rtos_e2e.rs`:

* the `MAX_ROUTER_SESSIONS` doc comment now carries the derivation above,
  replacing the per-node `2L` table that predicted 8 where 5 was measured;
* `PUBSUB_MIN_SAMPLES`'s "the lease values split cleanly" paragraph is corrected
  — the middle of the band is a beat, not a clean split;
* the assertion message no longer claims "any client lease under 30 s lapses
  deterministically";
* a new `COVERED_LEASE_SECS = 14` is PRINTED on every run, so a PASS states its
  own scope (issue 0445's rule, one layer down): *"this count can fail a lease of
  <= 14 s, and nothing above it"*. A verdict that does not report its scope cannot
  be caught having the wrong one, which is how this survived.

### The cheap alternative, priced and declined

Dropping `MAX_ROUTER_SESSIONS` from 3 to 2 needs only ONE lapse, which buys
`L <= 18 s` for **no wall clock at all** — the same band as doubling the cell,
free. Not taken: the slack exists for a single genuine re-dial, nine healthy runs
on an unknown subset of the twelve cells is not enough evidence to retire it on
all four platforms, and turning this cell flaky is worse than the four seconds of
lease band it buys. Whoever wants it should first measure the healthy session
count on all twelve cells across a few runs; if it is 2 every time, the change is
one constant.

### Still unavailable, as filed

Counting per NODE would make the slack "one re-open each". The client zid is
regenerated on every session open — `zpico.c`'s
`zpico_next_session_zid_counter()` mixes a monotonic counter and the clock into
it — so grouping the router log by zid reads as more NODES rather than more
sessions. Unchanged, and confirmed by reading the generator.

## Appendix — the model, so the table can be re-derived

```python
def first_close(lease_s, ka=30.0, horizon=100000.0):
    """Seconds from session open to the first close, per
    `_zp_unicast_lease_task`: wake every `lease`, consume `_received` if set,
    otherwise close. The handshake sets it at t=0; an idle router sets it every
    `ka` seconds."""
    received, t, k = True, 0.0, 0
    while t < horizon:
        prev, k = t, k + 1
        t = k * lease_s
        if int(t // ka) - int(prev // ka) > 0:
            received = True
        if received:
            received = False
        else:
            return t
    return None
```

`first_close(10) == 20.0`, matching issue 0906's measured ~20 s and the cell's
measured 5 sessions (`2 + floor(60 / 20)`). Sessions in a talker life `D` are
`2 + floor(D / first_close(L))`; the cell fails when that exceeds
`MAX_ROUTER_SESSIONS`.

## Not a regression

Nothing here was newly broken. What is now different is that the cell's coverage
is derived rather than asserted, and it says so at runtime.
