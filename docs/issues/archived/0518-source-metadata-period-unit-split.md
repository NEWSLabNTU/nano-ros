---
id: 518
title: "`SourceMetadata` could not be parsed at all — `deny_unknown_fields`
  rejected #505's added `period_us`"
status: resolved
type: bug
area: orchestration
related: [issue-0505, issue-0519, phase-307]
---

## Symptom

`cargo test -p nros-cli-core --test plan_pipeline_e2e` — two reds on `main`:

```
metadata_build_discovers_missing_sources ... FAILED
metadata_mode_build_emits_source_metadata_for_component ... FAILED

valid SourceMetadata: Error("unknown field `period_us`, expected one of
  `id`, `declaration_slot`, `period_ms`, `callback`, `callback_slot`")
```

Reproduced on a clean tree (`git stash` of all unrelated work), so not an
interaction with in-flight changes. `SourceTimer` is
`#[serde(deny_unknown_fields)]`, so this is a hard parse failure of the whole
sidecar, not a dropped field.

## Cause — and a correction to this issue's first draft

**As first written, this issue said #505 "changed the writer and not this
reader" and that the fix was to rename `period_ms` → `period_us`. That was
wrong**, and acting on it would have been worse than the bug.

`#505`'s own commit message states the design plainly: *"`EntityMetadata` gains
`period_us` (emitted in the metadata JSON, `period_ms` kept for existing
consumers, runtime prefers `_us` and falls back)"*. The change was
**additive**, and `write_timer_json` (`node_metadata.rs:1068-1069`) emits both
fields, exactly as designed.

So nothing was renamed and nothing needed migrating. The reader simply refused
a field it had not been told about, because `deny_unknown_fields` makes every
additive producer change a breaking one for that struct.

The first draft's evidence for a rename was the error message alone. Renaming
made the tests fail the *other* way — `unknown field 'period_ms'`, because
the committed fixture `source_metadata_talker.json` still (correctly) carries
the millisecond field. That inverted error is what exposed the misreading; the
rename was reverted before anything was committed.

## Fix

Add the field rather than swap it:

```rust
pub period_ms: u64,
#[serde(default, skip_serializing_if = "Option::is_none")]
pub period_us: Option<u64>,
```

`Option` + `default` for two reasons: the producer emits both so neither is
authoritative, and the hand-written fixtures under
`tests/fixtures/orchestration/` predate #505 and legitimately carry only
`period_ms`.

`plan_pipeline_e2e`: 3 passed, 0 failed.

## Why it survived review

`SourceTimer` has no field readers anywhere — it is only declared and held as
`Vec<SourceTimer>`. Nothing but a test that parses a real sidecar can catch a
break in it, and #505 had no reason to run the CLI's plan-pipeline suite.

The general shape is worth remembering: **`deny_unknown_fields` converts a
deliberately backward-compatible producer change into a hard failure in a
consumer nobody thought to look at.** The attribute is right for catching
typos in hand-written config; on a struct that reads a machine-generated
sidecar from a different crate, it makes every additive field a coordinated
change.

## Left open

The plan path still reads only the truncating field, so `nros explain` reports
a sub-millisecond timer as `0ms` — [issue
0519](0519-plan-timer-period-truncates-sub-millisecond.md). Separate because it
changes the plan schema and needs a decision about `SchedContext`'s sibling
`_ms` fields that belongs to #505.
