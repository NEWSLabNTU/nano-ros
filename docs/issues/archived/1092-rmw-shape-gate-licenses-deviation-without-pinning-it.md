---
id: 1092
title: "`rmw-abi-shape` licenses a deviation without pinning it — 7 of 24
  mutations left every RMW gate green, including a `void` return and a gutted
  slot"
status: resolved
type: bug
area: ci, rmw
related: [phase-393, phase-428, rfc-0089]
---

## What is true today

The drift CLAUDE.md records is **gone**, and this was re-derived independently
rather than taken from the tools' exit codes:

* the 88-symbol contract holds exactly — three `librmw_*_cpp.so` export
  byte-identical sets and `diff` against the recorded file is empty;
* an independent brace-matching parser of `rmw_vtable.h` finds 68 slots, and
  `diff` against `rmw-abi-shape.vtable_slots()` is empty;
* the map partitions the contract exactly: 63 vtable / 20 declined / 3 layer /
  2 global / **0 gap**, no orphan slot, no missing symbol.

`check_against_vtable` works in both directions — four mutations that attack it
are all caught.

## The defect

**The cross-check is narrower than the green light it produces.** Coverage,
computed from the tools' own tables:

| | slots |
| --- | ---: |
| signature compared AND args exactly enforced | **15** |
| in `ARG_DEVIATIONS`, which accepts *any* argument list | **39** |
| never signature-compared at all (11 `ADDED` + 3 grouped-only) | **14** |
| total | 68 |

`ARG_DEVIATIONS` has 42 entries and every value is a **reason string**. The
branch is

```python
elif slot in ARG_DEVIATIONS and (ret_ok or slot in RET_DEVIATIONS):
```

— membership, not content. The entry says *that* a slot deviates and never
*how*, so once a slot is listed, the header may say anything.

## Mutations that left all four commands green

Twenty-four applied to a scratch mirror; **seven survived**.

| mutation | why nothing saw it |
| --- | --- |
| **`has_data` gutted** — return → `void`, handle → `rmw_publisher_t *`, two junk args | it is in `ADDED`, which is checked for non-emptiness and non-empty reasons. Nothing asserts an `ADDED` slot EXISTS or keeps its shape |
| **`has_data` deleted entirely** | same hole; only `rmw-vtable-order` noticed, via positional-initialiser drift |
| **`create_node` return `rmw_ret_t` → `void`** | `RET_DEVIATIONS` membership |
| **`take` gains `uint64_t bogus_extra`** | `ARG_DEVIATIONS` membership |
| **`take`'s handle → `const rmw_publisher_t *`** | same |
| **`create_session` → `void(int, char)`** | grouped-only, never compared |
| **`subscription_take_event` → `void(rmw_publisher_t*, int)`** | same |

**The `void`-return one is a regression of the exact defect the gate exists
for.** W5 found six slots returning `void` where upstream returns `rmw_ret_t` —
"the axis that decides whether a caller can detect failure at all" — and
reintroducing it on any of the six `RET_DEVIATIONS` slots is now invisible.

`rmw-api-comparison` reddens for four of the seven, but it is a **staleness**
gate: it compares the rendered doc to the committed one, so regenerating the
doc clears it and the parity instruments stay green on a broken ABI.

## The unguarded exception mechanism

There are **zero `gap` rows today**, so clause 3 is vacuously satisfied. The
mechanism is not: the deferral rule matches `\bissue[ -]?(\d{4})\b` and never
resolves the number.

* a gap reason naming **issue 9999**, which does not exist → all four green;
* a gap reason naming **issue 0776**, which is `resolved` and archived → all
  four green;
* authored `status = "not-implemented", issue = 9999` → green; `issue = "banana"`
  → green (`check_status` tests truthiness only).

The exemplar cited in the shape script's docstring, its error text and its
self-test probes is itself issue 0776 — resolved and archived.

## Fix, ranked

1. **Assert every `ADDED` key is a live slot and pin its signature** (or at
   minimum its return type). Closes the two mutations nothing but bindgen saw.
2. **Make `ARG_DEVIATIONS` / `RET_DEVIATIONS` values carry the EXPECTED
   signature**, not a reason alone, so an entry pins the difference instead of
   licensing all of them. Raises exactly-enforced slots from 15/68 toward 54/68.
3. **Compare grouped-only targets** against the upstream signature of the
   symbol grouped onto them.
4. **Resolve every `issue NNNN`** in a gap reason and every authored `issue =`
   to a file in `docs/issues/` with `status: open`.

## Not verified

`check-abi-bindings` was not run (it writes `generated.rs` into the tree). A
scratch re-run of the pinned bindgen 0.72.1 against the mutated header produces
a different `generated.rs`, so it would go red — on a host that has bindgen; the
recipe skips when it is absent. The snapshot files were not mutated; the
contract half re-derives byte-identically on this Humble install and nothing is
claimed about other distros.

## Resolution — every declaration pins what it licenses (2026-09-11, phase-444 W4.a)

All four ranked fixes landed in `scripts/rmw-abi-shape.py`, plus the parity
map's half of fix 4 in `scripts/rmw-api-parity.py`.

1. **`ADDED` entries are `Added(ret, args, why)`.** Each key must be a live
   slot (else `orphan_pin`) whose return and argument list equal the pin (else
   `added_drift`), and an `ADDED` slot that turns out to have an upstream
   counterpart is refused. Closes both `has_data` mutations.
2. **`ARG_DEVIATIONS` / `RET_DEVIATIONS` entries are `ArgPin(upstream, ours,
   why)` / `RetPin(upstream, ours, why)`**, in the normalised spelling the
   comparison uses. A declared slot passes only while BOTH sides still read
   exactly the pin; any change to the header or to the upstream snapshot is
   `pin_drift` and prints the current shape to paste. There is deliberately no
   flag that rewrites pins — a gate you clear by regenerating its expectation
   is the staleness gate this issue found `rmw-api-comparison` to be. The
   stale-pin check (a pin on a slot that now matches) moved out of
   `--self-test` into `compare()`, so `--check` fails on it too, and a pin no
   comparison ever consults is `orphan_pin`.
3. **Grouped-ONLY targets are compared** against the upstream symbol grouped
   onto them: `create_session` ← `rmw_init`, `destroy_session` ← `rmw_shutdown`
   and `rmw_context_fini`, `subscription_take_event` ← `rmw_take_event`. The
   first two gained pins with reasons; the third's entry already existed and
   had never been read.
4. **Deferral ids resolve.** `scripts/lib/issue_status.py` is the one resolver;
   a `gap` reason defers only to an issue whose file says `status: open`, and a
   `not-implemented` row's `issue =` must do the same (`9999`, `0776` and
   `"banana"` are all refused now). The docstring/error exemplar is no longer
   the resolved 0776.

### What the pins found on the way in

* `ARG_DEVIATIONS["subscription_get_network_flow_endpoints"]` declared a
  deviation on a slot that has never existed (both network-flow symbols are
  `declined`, issue 0956), "as" a sibling entry that did not exist either.
  Deleted; `orphan_pin` is the check that would have caught it.
* `take`'s reason listed `(sub, buf, buf_len, size_t *out_len, bool *taken)`
  — five arguments, the pre-phase-406 shape — beside a header that takes
  three `(sub, rmw_mut_byte_span_t *, bool *)`. Reason corrected. That is the
  argument for pinning in one line: the prose had been false since phase-406
  and nothing could see it.

### Coverage, recomputed

| | slots |
| --- | ---: |
| identical to upstream | 15 |
| name matches upstream, difference PINNED | 39 |
| grouped-only target, difference PINNED | 3 |
| RTOS addition, signature PINNED | 11 |
| **compared with no licence to vary** | **68 of 68** |

### Acceptance (phase-428 W7: all seven surviving mutations fail)

Executable, not recorded: `--self-test` replays the seven mutations above
against the REAL header on every run (`MUTATIONS`), and fails if any one adds
no `--check` failure the unmutated tree lacks — `OK (71 slot(s) parsed, 28
case(s), 7 of issue 1092's mutations caught)`. The planted cases cover an
unchanged pin passing, an appended argument / a retyped handle / a `void`
return / upstream moving under a pin each failing, and stale ARG and RET pins
being reported.

Negative control on the real tree: `take` given a trailing `uint64_t
bogus_extra` in `rmw_vtable.h` → `just check rmw-abi-shape` rc=1 with

```
take: the args changed under a PINNED deviation
    pinned ours: ("const rmw_subscription_t *", "rmw_mut_byte_span_t *", "bool *")
    header now : ("const rmw_subscription_t *", "rmw_mut_byte_span_t *", "bool *", "uint64_t")
```

while `origin/main`'s copy of the script, run against the same mutated
header, printed `name matches, args DECLARED : 39` and exited 0. Header
restored.
