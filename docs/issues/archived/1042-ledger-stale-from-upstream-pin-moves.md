---
id: 1042
title: "Nine ledger rows went false because the rclrs pin moved 0.5.1 -> 0.7.0 and
  0.7.0 ships actions — a `--refresh` invalidates rows and nothing re-reads them"
status: resolved
type: bug
area: docs, api
related: [phase-482, phase-417, rfc-0087, issue-1012, issue-1022]
---

## Problem

`docs/reference/api-surface/rclrs.json` is a RECORDED surface, re-derived by
`api-parity.py --refresh`. Commit `d4024c0b0` re-pinned it from rclrs 0.5.1 to
**0.7.0**, and 0.7.0 ships actions — twelve `rclrs::Action*` records where 0.5.1
had none.

Nine rows argued from that absence, in the same words:

> rclrs 0.5.1 ships NO action API at all … the convergence point is whenever
> rclrs grows actions.

It grew them. The rows are now `extension` (we add it, ROS 2 has none) while
correlating `same` against a real upstream counterpart:

`rust:ActionClient`, `rust:ActionServer`, `rust:CancelResponse`,
`rust:GoalStatus`, `rust:QoSProfile::action_status_default`, and
`rust:Timer::{is_canceled, is_ready, time_since_last_call, time_until_next_call}`
(the last four now on `rclrs::TimerState`; the correlator folds the `State`
suffix).

`rclrs::CancelResponseCode` and `GoalStatusCode` also exist now, which bears on
`rust:CancelReturnCode`.

## The class, which is the point

Every staleness rule this campaign has written assumes WE moved. Issue 1012 is
prose naming a symbol our rename retired; issue 1022 is prose stating a false
fact about our code. Both are about our side.

**The ledger is a join over two moving surfaces.** A row that says "ROS 2 does
not have this" is a claim about THEIR side, and it expires when the recorded
surface is refreshed — with no edit to our tree, no failing gate, and nothing in
the diff of the refreshing commit to suggest it.

`--refresh` currently rewrites the surface and stops. It should also report
which rows argued from the absence of something the new surface now contains.

## Why the gate did not catch it

`--check` compares OUR surface to the recorded one and demands a row per
non-corresponding item. These rows HAVE entries — the entries are just wrong
about why. A verdict of `extension` on a row whose bucket is `same` is the
detectable shape, and it is the same contradiction class issue 1022 recorded
(`declined`+`ours-only`, `extension`+`theirs-only`): a verdict that cannot
coexist with its bucket.

## Fix

1. Re-verdict the nine against rclrs 0.7.0 — `divergence`, `rename` or delete,
   per row. This is a judgement call about shapes, not a mechanical sweep.
2. Gate the contradiction: `extension` may not sit on a row whose bucket is
   `same` or `theirs-only`; `declined` may not sit on `ours-only` UNLESS its
   disposition is `refuse-loud` (that exemption exists because a refusal is a
   declaration — RFC-0089). Three of the four shapes are already known to occur.
3. Make `--refresh` name the rows it may have invalidated: any row whose `why`
   mentions a symbol or family the NEW surface contains and the OLD one did not.
   That is computable from the two surface files at refresh time, which is the
   one moment someone is looking.

## Resolution (2026-10-07, phase-482 W7)

All three steps of the fix are done.

1. **Re-verdicted against rclrs 0.7.0**, read from its `action.rs` at the
   pinned commit:
   - `rust:ActionClient` and `rust:ActionServer` → `divergence` /
     `adopt-bounded`. rclrs's are `Arc<…State<A>>` aliases with async goal
     handling, while ours are fixed-size and executor-driven. The envelope
     sentence is now in each type's doc comment
     (`nros-node/src/executor/handles.rs`) and the rows cite it.
   - `rust:GoalStatus` → `rename`: our enum is their `GoalStatusCode`. Their
     `GoalStatus` is the struct `{code, goal_id, stamp}`.
   - `rust:CancelResponse` → `divergence`: their `CancelResponse` is the struct
     `{code, stamp}`, ours is the per-goal decision.
   - `rust:CancelReturnCode` now names rclrs's `CancelResponseCode`.
   - `rust:QoSProfile::action_status_default` and the four `rust:Timer::*` rows
     correlate `same` and needed no row, so they are deleted.
2. **The contradiction is gated** (`contradictory_verdicts` in
   `scripts/api-parity.py`):
   - `extension` on `same` / `theirs-only` is red;
   - `declined` on `ours-only` is red unless it is `refuse-loud`.

   Six self-test cases. It found exactly these nine rows on main.

   One side fix: `same_shaped_divergences` now skips an enum-versus-struct pair
   (`kinds_never_correspond`). The compiler always reports that pair, so asking
   for a disposition there was asking about a silence that cannot occur.
3. **`--refresh` names the rows the new surface may invalidate**
   (`rows_citing_new_names`). These are rows whose `why` cites, in backticks,
   the last segment of a newly recorded name. It is advisory; step 2 is what
   fails afterwards.
