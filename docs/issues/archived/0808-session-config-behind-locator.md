---
id: 808
title: "`create_session`'s flat argument list cannot carry session config, and the
  structural fix has been deferred twice into issues that are now closed"
status: open
type: tech-debt
area: rmw
related: [issue-0785, issue-0331, issue-0330, issue-0800, phase-376]
---

## Problem

`create_session(locator, mode, domain_id, node_name, out)` is a flat argument
list, so anything else a session needs at init time requires an ABI break to
add. Three things want in and cannot get in:

| want | why it cannot be added today |
| --- | --- |
| `mode` (already there) | backend-SHAPED — it is zenoh's `whatami`, and cyclonedds/XRCE are told to ignore it. It should not be on an agnostic vtable at all |
| `localhost_only` | a real discovery-scope control; maps to a cyclonedds setting. Issue 0785 |
| `enclave` | issue 0785, and its absence makes `rmw_get_node_names_with_enclaves` a HOLLOW grouping — the visitor's enclave argument is structurally always NULL |

The agreed shape has been the same each time it was written down: fold
backend-private session config behind the locator, so the agnostic vtable stops
carrying backend-shaped fields and gaining a field stops being an ABI break.

## The actual finding: the deferral chain is dangling

Nobody is tracking it. Both issues that deferred it have been resolved, and
neither did it:

- **Issue 0331** documented the `mode` enum and said in its own resolution:
  *"The structural half — folding the mode into backend-private config behind
  the locator, so the agnostic vtable stops carrying a backend-shaped field —
  is NOT done. It is the same class as issue 0330 and is better done alongside
  that issue's part 3."* Status: **resolved**.
- **Issue 0330 part 3** turned out to be about something else. Its
  investigation changed the shape of the fix to "move the force-link anchor to
  the layer that legitimately names a backend" (`nros::force_link_backend!`).
  Nothing about session config or the locator. Status: **resolved**.
- **Issue 0785** then deferred `localhost_only` and `enclave` to 0331 — by then
  already closed.

So `rmw_vtable.h` currently tells a reader "see issue 0331" for a fix that
issue explicitly did not perform and that no open issue owns. A deferral to a
resolved issue is indistinguishable from a deferral to a plan, which is how
three separate needs came to point at nothing.

This issue exists to be that home. It is deliberately not a promise about
shape — it is a promise that the shape has an owner.

## Direction

1. Decide the carrier. The locator is the natural one (it is already a
   backend-interpreted string and already the only discovery-shaped input), but
   a second opaque `config` argument taken ONCE is also on the table; the
   trade is "parsing in every backend" against "one more ABI break, then never
   again".
2. Whatever it is, it carries `mode`, `localhost_only` and `enclave` together.
   Doing one of the three alone spends the ABI break without retiring the
   problem.
3. `localhost_only` needs a real backend implementation to be worth carrying —
   cyclonedds is the one that can honour it. A carried field no backend reads
   is an inert slot in a different costume (issue 0800).
4. When it lands, `rmw_get_node_names_with_enclaves` stops being hollow, and
   the per-symbol note in `rmw-api-parity.py` should show it moving out of the
   inert column rather than being asserted fixed.

## Not in scope

`security_options` stays declined, and the reason is about the target rather
than about this argument list: it is a DDS-SROS2 keystore PATH plus an
enforcement switch, and neither a filesystem nor a DDS security plugin exists
where this ABI runs. `allocator` and `instance_id` likewise stay out —
declined ABI-wide, and rcl-side process identity, respectively.
