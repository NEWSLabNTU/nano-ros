---
id: 1049
title: "`nros_timer_get_time_until_next_call` returns 0 for every registered timer,
  and correlates `same` against rcl so the ledger has no row watching it"
status: resolved
type: bug
area: api
resolved: 2026-09-11
related: [phase-417, issue-1008, rfc-0087, rfc-0089]
---

## Resolution (phase-417 stage 3, 2026-09-11)

Fixed as proposed, with ONE change to the proposed signature: the out-param is
`int64_t *`, not `uint64_t *`.

```c
nros_ret_t nros_timer_get_time_until_next_call(const nros_timer_t *timer,
                                               int64_t *time_until_next_call_ns);
```

The ledger row `c:timer_get_time_until_next_call` — written after this issue,
and now deleted — recorded the third loss this issue did not: rcl's header says
"a negative value indicates the timer call is overdue by that amount", so
unsigned cannot carry lateness and reports it as `0`, which is also what "fires
now" reports as. Upstream's type is `int64_t` and so is ours.

The body forwards to `Executor::timer_period_us` + `Executor::timer_elapsed_us`,
the same arena accounting W5.c's siblings read, and the caller-supplied "now" is
gone. That inherits the sibling accessors' microsecond-resolution envelope
(issue #505), which is stated on the function.

The section above is otherwise unchanged and still correct — including its
general point, which RFC-0089 has since adopted: **`same` is a claim about the
signature, never about the behaviour.** The row this issue said did not exist
was filed by phase-428's by-hand sweep, which is the answer to "nothing was
watching it".

## Problem

`nros_timer_get_time_until_next_call` (`packages/api/nros-c/src/timer.rs:312`)
computes its answer from `timer->last_call_time_ns`. That field is written by
exactly two places — `nros_timer_init` and `nros_timer_reset` — and **no
dispatch path ever updates it**.

So for any timer the executor is actually running, `last_call_time_ns` stays at
its initial value forever, and the function returns `period - current_time_ns`
saturated to zero: **0 for any plausible clock reading**. The accessor reports
"fires now" permanently.

It also returns `0` for "invalid", so a caller cannot tell an unanswerable
question from a due timer. That is issue 1008's exact shape — a value channel
carrying an error — one accessor over.

## Why nothing caught it

The name matches `rcl_timer_get_time_until_next_call`, so the correlator buckets
it `same` and **the ledger has no row for it**. The ledger only holds rows for
items that do NOT correspond, so a function that is broken but correctly *named*
is invisible to the whole parity apparatus.

That is worth stating as a general limit rather than a one-off: **`same` is a
claim about the signature, never about the behaviour.** Everything this campaign
has built — verdicts, dispositions, the `--require-disposition` gate — hangs off
rows that exist, and a correctly-named wrong implementation creates no row.

The related trap is already recorded in RFC-0089 (a refusal correlates `same`;
a sentinel correlates `same`); this is the third member of that family and the
only one where the code is simply wrong.

Found while implementing W5.c's sibling accessors, not by any gate.

## Fix

Forward to the executor, as W5.c's `nros_timer_get_time_since_last_call` now
does: `Executor::timer_elapsed_us` (`nros-node/src/executor/spin.rs:6003`) reads
the arena's own accounting, which is the state the dispatcher actually advances.

That is a signature change, and it should take the house shape rather than
patching the body:

```c
nros_ret_t nros_timer_get_time_until_next_call(const nros_timer_t *timer,
                                               uint64_t *out_ns);
```

— dropping `current_time_ns` (the executor knows the time; a caller-supplied
"now" is what let the two time bases drift apart in the first place) and moving
the answer to an out-param so "unanswerable" stops being spelled `0`.

`nros_timer_get_next_call_time` — rcl's ABSOLUTE instant — stays unimplemented
and is a separate question: we keep no absolute activation time, and deriving
`now + (period - elapsed)` composes two time bases. W5.c refused it for that
reason and the refusal stands.

## Blast radius

No in-tree caller. The signature change is therefore free now and will not be
later.
