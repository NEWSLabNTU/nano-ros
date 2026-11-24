---
id: 2954
title: "The book-probe helpers log a step to a fixed `/tmp/probe-run-<step>.log`, so two gate lanes running the helper self-test at once read each other's output"
status: resolved
type: bug
severity: medium
area: [testing, ci, book]
related: [issue-2933]
found: 2026-11-24
resolved: 2026-11-24
---
## What
`just ci gate` failed in `gate::check::pristine-checkout`, whose inner
`check fast` failed `check::starter-image-context`:

```text
AssertionError: ('echo done; exit 0', '0', '',
  'Error: refused\nPROBE FAIL: step 99 exited 0 as documented but did not print: done\n')
```

The case that must print `done` found `Error: refused`, which is the output of
another case in the same table.

## Why
`nros_probe_run_exit` and `nros_probe_run_expect` (`scripts/probe/probe-run.sh`)
log step N to `/tmp/probe-run-N.log`. That is fine inside a probe container,
which runs one chapter. But `extract-book-steps.py`'s self-test (since issue
2933) runs the helper on the HOST as step 99. A gate runs `check fast` twice at
the same time, once in its own lane and once in the pristine-checkout clone,
so the two self-tests write and read the same file.

## Fix
Both helpers log under `NROS_PROBE_LOG_DIR`, defaulting to `/tmp` (the probe
container is unchanged). The self-test points it at each case's own temporary
directory.

## Acceptance
`check-starter-image-context` passes; two concurrent self-tests share no
log path.

## Follow-up: the log path is structural (2026-11-25)
The first fix (PR #4809) put the log under a `NROS_PROBE_LOG_DIR` the CALLER
had to set, defaulting to the shared `/tmp`. A future concurrent caller that
did not set it (a second self-test case, `nros_probe_run_expect` driven from a
test) would bring the race back. Following nros 118's review, both helpers now
write the log beside the body file, `${body%.sh}.log`. Every caller already
makes that file unique: `/tmp/probe-run-<step>.sh` in the probe container
(so the same log path as before) and a per-case tempdir in the self-test. The
knob is removed, so there is one mechanism.

Measured: `extract-book-steps.py --self-test`, 8 parallel runs x 3 rounds:

| Tree | Red |
| --- | --- |
| before the fix (main at 2159f9fbd0) | 4 of 24 |
| PR #4809 (the knob) | 0 of 24 |
| this follow-up (log beside the body) | 0 of 24 |

nros 118's independent run (`check-starter-image-context.py`, 8 parallel):
main 5 of 8 red, #4809 8 of 8 green.
