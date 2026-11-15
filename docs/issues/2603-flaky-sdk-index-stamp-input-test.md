---
id: 2603
title: "`configure_stamp::tests::the_sdk_index_is_a_stamp_input` fails under a full gate and passes alone"
status: open
type: bug
area: [testing, cli]
severity: medium
found: 2026-11-15
related: [phase-478, issue-2598]
---

## What happens

In one `just ci gate` run (`check::build`, the `check-cli-tests` lane) on the
issue 2598 branch, this test failed at its second assertion:

```text
panicked at nros-cli-core/src/configure_stamp.rs:1613:9:
assertion failed: matches!(is_current_with(p, "img", &real_env(), None).unwrap(),
    Currency::Stale(why) if why.contains("outside the project") &&
    why.contains("changed"))
```

The same tree then passed it 3 of 3 times alone, and the whole
`nros-cli-core` lib suite (2487 tests) passed. The host was busy at the time
(other cargo builds running), and the branch's own change does not reach the
code path (the key is `img`, not a workspace `<bringup>:<image>` key).

A red that reproduces only in a full lane costs every agent a gate re-run.

## What is known

- The assertion before it (`Currency::Current`) passed, so the recorded
  external read was there and current.
- `external_moved` compares content digests, not mtimes, so a file timestamp
  cannot explain it.
- The external-read recorder is thread-local (`begin_external_reads`), so a
  concurrent test cannot add reads to this one's record.
- The panic does not say what `is_current_with` returned instead: the
  `matches!` hides the reason, so the cause is still unknown.

## Next step

Make the assertion print the actual `Currency` (`assert!(matches!(..), "{got:?}")`)
so the next failure names its reason. Candidates to check against that
reason: a check `is_current_with` runs before the external reads and that
reads shared state (the process environment through `real_env`, the CLI's
planning digest, the codegen version), and the fixed scratch-dir names that
`tmp()` reuses across runs.
