---
id: 2410
title: "The RMW C shim maps a failing backend poll to 'no data': `has_data`, `has_request`, `next_deadline_ms`, both status-event polls and the in-place probe fold the status into the flag by hand, so a broken backend reads exactly like a quiet one"
status: resolved
type: bug
severity: medium
area: [rmw, executor, diagnostics]
related: [2403, phase-547, 0737]
found: 2026-11-10
resolved_in: "the issue-2403 pull request: one helper, `status_report::status_ok`, at all six sites, and gate `check-cffi-swallowed-status`"
---

**Found by [issue 2403](2403-px4-uorb-subscription-never-taken.md).**

## What

`nros-rmw-cffi` adapts C vtable slots that return a status plus an out-flag
to trait methods that answer `bool` or `Option`, with no error channel.
Six sites wrote the adaptation by hand, `rc == NROS_RMW_RET_OK && has` or
`if rc != NROS_RMW_RET_OK || !taken { continue; }`:

| site | answer on error |
|---|---|
| `Subscription::has_data` | no data |
| `ServiceTrait::has_request` | no request |
| `Session::next_deadline_ms` | no deadline |
| publisher `poll_status_events` (`publisher_take_event`) | no event |
| subscription `poll_status_events` (`subscription_take_event`) | no event |
| `subscription_supports_in_place` probe, at create | not supported |

Each answer is the one a healthy, idle backend gives. Issue 2403 is what it
cost: on NuttX every `orb_check` failed with EBADF, `has_data` answered "no
data" on every pass, and the subscription silently never delivered. The first
measurement read it as "the executor never calls `take`".

## Fix

`packages/rmw/cffi/src/status_report.rs`: `status_ok(rc, &SITE)` still answers
`rc == NROS_RMW_RET_OK` (the traits cannot carry more), and reports the first
error each site sees through `nros_log`, naming the slot and the status. Once
per site, not per call, since these run on every executor pass. All six sites
use it.

Gate: `just check cffi-swallowed-status` (`scripts/check-cffi-swallowed-status.py`,
self-test on every run) refuses a comparison against `NROS_RMW_RET_OK` joined
with `&&` or `||` anywhere in `packages/rmw/cffi/src/`. Against the tree
before the fix it names exactly the six sites above.

Not in this class, and left as they are: `register_waker` (a status alone,
whose `false` means "poll instead", which is the caller's documented
fallback) and `read_granted_qos` (a non-OK return is a declared absence,
documented at the function).
