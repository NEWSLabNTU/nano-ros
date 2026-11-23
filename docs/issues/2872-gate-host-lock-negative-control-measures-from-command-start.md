---
id: 2872
title: "gate-host-lock's \"waiter is out before the holder's timeout\" control measures from the holder's command start, which lands late under load"
status: resolved
type: bug
severity: low
area: [ci, testing]
related: [issue-2602]
found: 2026-11-23
---

## What happens

`tests/gate-host-lock-tests.sh` case 4 starts a holder with a 2 s wall limit,
then a waiter, and asserts the waiter started more than 2 s after the holder:

```sh
gt "$(cat "$tmp/b4.start")" "$(python3 -c "print($(cat "$tmp/a4.start") + 2)")"
```

`a4.start` is written by the holder's command (`bash -c "echo $(date …) > …"`),
which runs inside the deadline wrapper (`scripts/ci/gate-host-lock.sh`,
`"${NROS_DEADLINE[@]}" "$gate_s" "$@"`). The 2 s are counted from when that
wrapper starts, and the command writes its timestamp some time after it. On a
loaded host that delay is large, and the waiter correctly gets in at
"wall start + 2 s", which is before "`a4.start` + 2 s". The control fails
although the lock behaved as it should.

Measured 2026-11-23 in `just check fast` of PR #4628 at 83cd7fa096, with 36
runnable processes on the host: `FAIL negative control: the waiter is out
before the holder's timeout`, and every other case in the script passed. The
same lane passed on the previous run.

## Fix

The bound is now taken from `a4.launch`, a timestamp the test records before
it launches the holder. That is no later than the moment the holder's wall
limit starts, so "the waiter started more than 2 s after it" is a lower bound
the lock guarantees and load can only widen. A broken lock that let the waiter
straight in still fails it.

## Resolution

Fixed in the test, as above.
