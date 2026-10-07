---
id: 1323
title: "The API-parity ledger has no stale-row detection, so a row survives the
  entity it describes — ten did, and one was reported as a fresh unledgered
  difference the day the masking overload was deleted"
status: resolved
type: bug
area: [ci, api, docs]
related: [phase-482, phase-379, phase-417, phase-428, phase-442, rfc-0089, rfc-0096, 0196, 1204, 1225]
---

## Problem

`check-api-parity` is a ratchet in one direction only. A difference with no
ledger row FAILS, which is the arm that stops the debt growing. A ledger row
naming an entity that no longer exists on EITHER side passes silently, which is
the arm that would stop the debt going stale — and it is missing.

Measured on phase-442 W2, the commit that deletes `std_compat.hpp`. That header
held eleven free functions. After deleting it the gate was **green**, while ten
ledger rows described entities the tree no longer contains:

```
cpp:create_action_client        cpp:executor_spin
cpp:create_action_server        cpp:executor_spin_for
cpp:create_executor             cpp:executor_spin_once
cpp:create_guard_condition      cpp:get_fully_qualified_name
cpp:create_subscription *       cpp:create_timer_oneshot
```

They were found by hand, by listing the deleted header's symbols and grepping
the ledger for them — which is exactly the check a gate should be doing, and
exactly the check nobody will run next time.

(*) `cpp:create_subscription` is in that list because the hand-check got it
WRONG, which is the second half of the problem. It sits in the `differs`
bucket, which neither `--show all` nor `--show same` prints, so a probe built
from those two views reported it dead and it was deleted. The gate caught that
immediately — `1 item(s) differ with no ledger entry` — and the row went back.
So the ledger's two halves have opposite failure modes today: deleting a LIVE
row fails loudly and instantly, and keeping a DEAD one costs nothing.

## Why a dead row is not harmless

This repo has already ruled on the class, twice, in the two other ratchets:

* `.config/cpp-capability-layout-baseline.txt` — "a line here that no longer
  matches what the gate measures is a FAILURE, so the debt cannot go stale",
  with a selftest case that proves the arm fires (issue 1225 §4, case 4).
* `.config/cpp-freestanding-includes-baseline.txt` — same shape, same reason.

The argument in both is that a stale exemption ABSORBS the next real violation
in that subject. It applies here with one extra turn of the screw: a ledger row
is not only an exemption, it is a WRITTEN CLAIM about what a porting user gets.
Nine rows in this tree said "it is the `NROS_CPP_STD` `std::string` overload in
`std_compat.hpp`, so a `no_std` consumer does not get this spelling" about a
header that no longer exists. A reader deciding how to port a file would have
believed them. The four of those nine whose KEY survives were amended in the
same commit — and it was `check-ledger-orphan-refs`, not a human, that found
every one of them (see below).

Two rows also carried `see \`cpp:create_publisher\`` against a key that had
never existed — a dangling cross-reference, the same defect one level down, and
also invisible today. The row exists now, authored by phase-442 W2 for an
unrelated reason, so those two stopped dangling by luck.

## Why nothing caught it

Issue 0196's shape, for the third time in this area: the gate's REACH is
narrower than the rule it enforces. `--check` walks the DIFFERENCES and asks
each whether the ledger has a row. Nothing walks the LEDGER and asks each row
whether the difference still exists. The rows are authored, so they drift in
the safe-looking direction, which is the same reason the RMW parity map read
`("gap", "no vtable slot")` for 28 slots W4 had landed.

## What would close it

The reverse walk, in `--check`: every ledger key must correspond to an item the
extraction produced, in ANY bucket, and a key that does not is a failure naming
the row to delete. Two details the hand-check above proves are load-bearing:

1. **"In any bucket" must mean every bucket the extractor computes**, not the
   ones a `--show` view happens to print. `differs` is not in `all` and not in
   `same`, and building the check on the printed views reproduces the exact
   wrong answer this issue was filed from.
2. **A negative control on the normal path**, like the other two ratchets have:
   a synthetic ledger row naming an entity nobody has must fail, and the
   unmutated ledger must pass first, so a broken harness cannot be mistaken for
   a caught mutation (issue 1204's case).

The FILE half of this is already built and already works, which is the best
argument that the key half is affordable. `check-ledger-orphan-refs` walks every
`why` for a cited repo path or bare filename and fails when nothing resolves,
with the useful refinement that a sentence saying the file was deleted or
retired is allowed to name it. It caught all ten `std_compat.hpp` citations in
this very commit, instantly, including the four in rows whose KEY survives —
which is exactly the reach the key walk is missing. Ten selftest cases back it.

So the ask is narrow: the same walk, over keys instead of filenames, with one
more selftest case. Cross-references are a free third: a `` `cpp:x` `` inside a
`why` that names no key is the same class and the same walk.

## Repro

```sh
# the green-with-ten-dead-rows state, on phase-442 W2 before the hand-cleanup
just check api-parity            # OK

python3 - <<'EOF'
import json, pathlib
dead = ["cpp:create_executor", "cpp:executor_spin", "cpp:get_fully_qualified_name"]
for p in pathlib.Path("docs/reference/api-parity-ledger").glob("*.json"):
    for k in json.loads(p.read_text()):
        if k in dead:
            print(p.name, k)   # rows for functions the tree does not define
EOF
```

## Resolution (2026-10-07, phase-482 W7)

`scripts/api-parity.py --check` now walks the ledger as well as the
differences (`orphan_rows`). Every key must name something the extraction
produced, on either side and in any bucket, including the theirs records the
public-surface filter drops. A glob row must match at least one key. Only the
languages a run extracted are judged.

A row the extraction does not back is deleted, unless it says why it is kept,
as a non-empty string (SCHEMA.md):

- `retired`: the name is gone and the row records the rename.
- `unextracted`: the thing exists but the extractor cannot see it (a macro,
  struct field, enum variant, derive method or concept row).

A marker on a row the extraction DOES back is red too, so an exemption cannot
go stale.

On the day it landed, the walk found **97** unbacked rows on main, plus the 23
that phase-482 W6 retires. The 97 went as follows:

| action | rows | which |
| --- | --- | --- |
| deleted | 32 | the C zero-init rows the extractor spells the rcl way (`get_zero_initialized_*`, `same`, so never consulted), C++ methods that moved to `PollSubscription`/`PollClient` where the new rows already existed, `Node::pump`/`own_entity`, and the seven `BoardTransportConfig` setter rows |
| `retired` | 46 | deprecated aliases deleted in W-B5 and earlier, the old `log`-crate `Logger` methods, `ParameterServer`, `BoardTransportConfig` |
| `unextracted` | 18 | the `RCLCPP_*` and `nros_*!` macros, struct fields, an enum variant, a derive glob, a concept row, and a theirs type absent from the recorded surface |
| re-keyed | 1 | `rust:ActionServer::for_each_active_goal` → `ActionServerHandle::` |

Before deleting, the reasoning in eight of those rows that other rows pointed
at ("substantive verdict at `cpp:Client::call`", "see
`c:publisher_get_zero_initialized`") was carried into the surviving rows, and
the references were rewritten.

Not done: checking `` `lang:key` `` cross-references inside a `why` (this
issue's "free third"). Measured after the cleanup, the only references to a
deleted key sit in rows that themselves say the old row was deleted or moved.
