---
id: 429
title: "nano2nano GID/sequence tests grep listener trace output the binary no longer emits"
status: open
type: bug
area: testing
related: [issue-0422, issue-0157, issue-0164]
---

## Symptom

```
FAIL  nano2nano::test_gid_consistency
      Need at least 2 GID values to verify consistency, got 0
FAIL  nano2nano::test_sequence_number_increment
      Need at least 2 sequence numbers to verify increment, got 0
```

Deterministic — two consecutive runs, identical result. Not a flake.

## Cause

Both tests spawn the native Rust listener with `RUST_LOG=trace` and parse
`MessageInfo` trace lines out of its stderr for GIDs and sequence numbers
(`nano2nano.rs:201-205`). The listener emits none of it:

```console
$ zenohd --listen tcp/127.0.0.1:47449 &
$ NROS_LOCATOR=tcp/127.0.0.1:47449 RUST_LOG=trace \
    examples/native/rust/listener/target/nros-relwithdebinfo/listener
# grep -icE 'gid|MessageInfo|Waiting for'  →  0
```

"got 0" is therefore literal: the parse found nothing because there was nothing
to find. The transport is fine — the talker publishes normally against the same
router:

```console
$ NROS_LOCATOR=tcp/127.0.0.1:47449 examples/native/rust/talker/…/talker
nros: session open
Publisher data keyexpr: 0/chatter/std_msgs::msg::dds_::String_/TypeHashNotSupported
```

This is the grep-drift class CLAUDE.md names: an example's output was slimmed
and the tests that grep it were not updated (archived issues 0157 / 0164 are the
same shape, which is why the rule says to diff the grep pattern against what the
fixture actually prints BEFORE debugging delivery).

Note the test also waits for a `"Waiting for"` readiness pattern that is
likewise absent — so the wait silently times out and the run proceeds with no
listener output at all.

## Fix

Decide which side is authoritative:

- If `MessageInfo` tracing is meant to exist, it regressed in the listener and
  the bug is there.
- If it was deliberately removed, these tests need a different observation
  channel. Per CLAUDE.md they should assert on
  `nros_tests::output::*` constants rather than literal strings, so the next
  banner change breaks the constant rather than ten greps.

Either way the readiness pattern needs the same treatment, and a test that waits
for output that never comes should FAIL on the timeout rather than continue into
a confusing assertion.

## Notes

Found triaging issue 0422. Two of that issue's ~19 failures are this. Not caused
by phase-336 — the same tests fail on a fresh clone.
