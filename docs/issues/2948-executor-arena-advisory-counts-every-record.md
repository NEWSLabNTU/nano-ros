---
id: 2948
title: "executor_arena_advisory's one-shot check counts every log record, so any record on the second spin fails it under load"
status: open
type: bug
area: [testing, executor]
severity: low
found: 2026-11-24
related: [0900]
---

## What happens

`nros-tests::executor_arena_advisory::first_spin_reports_an_over_provisioned_arena`
failed once in a full `just ci gate` on host 142 (issue 2928's branch, which
changes only `nros-cli-core`; load average about 150):

```
assertion `left == right` failed: the arena advisory repeated on a second spin; it is one-shot
```

It passed 3 of 3 runs alone on the same tree:
`cargo test -p nros-tests --features trigger-test,component-runtime-test,phase216-substrate,signal-fd-wake-test --test executor_arena_advisory`.

## Why

The test's sink pushes EVERY record into `CAPTURED`, and the one-shot
assertion compares `CAPTURED.len()` before and after a second
`spin_once`. A record that is not the advisory, such as a zenoh session or
lease line that a loaded host makes more likely, reads as "the advisory
repeated". So the check is about all logging, not about the advisory flag.

## Fix direction

Count only the advisory's own records, matched on its text (an
`nros_tests::output` constant, per the test-grep rule), before and after the
second spin. Keep the assertion that the first spin emitted exactly one.
