---
id: 1463
title: "`stale_gaps()` exempts EVERY `gap` that carries a disposition, so a
  closed gap with one is unreachable by any gate — the reach is wider than the
  rule, and `c:log_severity_t` sat in the work queue for twelve days saying
  LANDED and FIXED"
status: open
type: tech-debt
area: [api, tooling]
severity: medium
found: 2026-09-23
related: [0196, 1040, 1226, 1188]
---

## The rule

`scripts/api-parity.py`'s `stale_gaps()` exists because a `gap` is written
against a `theirs-only` key, and when somebody ships the name the key moves to
`same` and the row silently stops being true. Nothing asks: a `same` row needs
no ledger entry, so `--check` never looks at the one it has. When the gate
landed (phase-444 truth pass) it found **18 closed gaps still counted in the
campaign's work queue**.

## The reach

```python
    for r in rows:
        bucket = r.get("native_bucket") or r.get("bucket")
        if bucket not in ("same", "systematic"):
            continue
        entry = ledger.get(ledger_key(lang, r["key"]))
        if entry is None or entry.get("verdict") != "gap":
            continue
        if entry.get("disposition"):
            continue          # <- this line
        out.add(ledger_key(lang, r["key"]))
```

The exemption has a good reason, stated in the docstring: phase-428
deliberately filed BEHAVIOUR defects as `gap` on names we share with upstream —
`c:executor_spin_some` compiles and returns `TIMEOUT` where rclc returns `OK` —
and each of those carries the disposition RFC-0089 asks of a same-shaped
difference. Those rows are true and must not be flagged.

But the exemption is **unconditional**, so its reach is *every dispositioned
gap*, which is wider than the rule it implements. Since phase-417's
gap-verdict pass gave a disposition to essentially every remaining `gap`, the
gate's live reach on a `same`-bucket row is now close to zero. Measured on
`main` at `fcba471be`: of the 22 `gap` rows in the ledger, **22 carry a
disposition**, so `stale_gaps()` can currently flag none of them.

This is the issue-0196 shape the repo keeps paying for, and the third one this
month after 1226 (`check-default-gates-run-somewhere` scoped to `just check`
names when the rule was the whole `ci gate` lane) and 1040.

## The measurement that found it

`c:log_severity_t` (`docs/reference/api-parity-ledger/log.json`), verdict
`gap`, disposition `adopt-bounded`, bucket `same` on both surfaces. Its `why`
opened:

> phase-417 stage 3 (2026-09-11) — LANDED, and reduced to ONE stated envelope.
>
> FIXED, both halves this row recorded. …

Three paragraphs of LANDED / FIXED, then one clause of STILL OWED at the end.
A reader acts on the first thing they read, and the campaign's queue counts the
verdict. It sat that way for twelve days and nothing could ask, because the
disposition exempted it.

Worse, the one paragraph that justified keeping the row was itself wrong about
the tree. It claimed "the envelope is stated on `nros_log_severity_t::to_facade`
and in the header". Measured 2026-09-23:

- `packages/api/nros-c/include/nros/log.h` named inheritance nowhere — `grep -i
  "unset\|inherit"` returned only the numbering paragraph and the enumerator.
- `packages/api/nros-c/src/log.rs:81` asserted the **opposite** of the
  envelope: resolving `UNSET` to the floor "is rcutils's own treatment of
  `UNSET`, which is numerically its floor". It is not. `/opt/ros/humble/
  include/rcutils/rcutils/logging.h` documents `UNSET` as INHERIT —
  `rcutils_logging_set_logger_level(name, UNSET)` unsets the level and
  `rcutils_logging_get_logger_effective_level` then walks the dotted ancestry
  up to `g_rcutils_logging_default_logger_level`.

Both are corrected in the phase-444 re-read commit that files this issue, so
`adopt-bounded` is now true of the code rather than of the ledger's description
of it.

## Why it matters

`adopt-bounded` is the only disposition that makes a claim about something
OUTSIDE the ledger — that a doc comment states an envelope. Nothing gates that
the comment exists, and nothing gates that it is true. So a dispositioned `gap`
can be wrong in two independent ways at once (the work is done; the envelope is
not stated) and be reachable by no check in the tree.

## What this is NOT

- **Not "delete the exemption".** Doing that flags every phase-428 behaviour
  row, which is ~all of them, and a gate that fails on everything the day it
  lands is one somebody switches off — the same argument `--require-disposition`
  was staged around.
- **Not a prose regex.** "Flag a `gap` whose `why` contains LANDED or FIXED"
  would have caught this row and will not survive contact with the next one.
- **Not `check-default-gates-run-somewhere`.** `stale_gaps` RUNS on every
  `just check api-parity`; it runs and answers nothing, which is issue 1158's
  distinction one layer over (a lane that ran vs a lane with signal capacity).

## What would close it

Some check that has signal on a dispositioned `gap`. Candidates worth pricing,
none of them obviously right:

1. **Make `adopt-bounded` prove its envelope.** A row with that disposition
   names the file:symbol whose doc comment carries it, in a field, and a gate
   asserts the symbol exists and its doc mentions the ledger key. That would
   have caught the false half of this row. It does not catch a stale LANDED
   paragraph.
2. **Date the still-owed clause.** A dispositioned `gap` carries a structured
   `owed` field rather than a paragraph, and `stale_gaps` flags a row whose
   `owed` has not been re-measured within N days. Turns a prose problem into a
   staleness problem, which the repo already knows how to gate.
3. **Split the verdict.** A behaviour defect on a shared name is not the same
   claim as an absence; if it had its own verdict, `gap` could go back to
   meaning "absent" and the exemption could go away entirely. The most
   invasive and the most honest.

Whichever lands, the acceptance is the same: a closed dispositioned `gap`
planted in a shard must make a gate red.
