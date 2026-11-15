---
id: 2613
title: "`weak_symbol_audit::owned_weak_symbols_are_audited` hits nextest's 60 s timeout on the hosted tier1 zephyr shard"
status: open
type: bug
area: [ci, testing]
severity: medium
found: 2026-11-15
related: [phase-559, 0050, 1543]
---

**Owner:** unassigned. Found by phase-559 W12's acceptance dispatch, and not
caused by it.

## What happens

`merge-runtime.yml`, lane=tier1, run 43716062858, head 1d968501f6. This was
the first run in which the hosted zephyr shard got through its run stage:
W12 evicts the metadata probe's outputs first. Once there, it had exactly
one real failure:

```
SLOW [> 30.000s] nros-tests::weak_symbol_audit owned_weak_symbols_are_audited
TIMEOUT [  60.019s] (3608/3655) nros-tests::weak_symbol_audit owned_weak_symbols_are_audited
```

The same test, at the same head:

| Where | Wall |
|---|---|
| Hosted tier1 `threadx_linux` shard, same run | 31.2 s (slowest list, #3) |
| Hosted tier1 `zephyr` shard | > 60 s, killed |
| Local 20-core host, under a concurrent gate (load 24) | 9.1 s, 4.95 s user |

The test runs `bash scripts/check-weak-symbols.sh`, which reads
`git ls-files` with no pathspec (issue 1543's reach fix) and greps every
tracked file. Its work does not depend on build output, so W12's eviction
cannot have changed what it reads. The shard's test step sets no per-test
override, so the default `slow-timeout = { period = "30s", terminate-after = 2 }`
applies.

## Why it matters

A tier1 zephyr verdict is now red because of a whole-tree source scan
on a 4-core hosted runner, not because of a defect. Until W12, this shard
never reached its run stage, so this is the first time the lane could
report it. It is the load-sensitive shape the team rule warns about. Do not
fix it with a retry.

## Expected

The audit's verdict should not depend on runner load. Measure where the 31–60 s
goes on the hosted runner (the per-file greps, or contention with the
shard's parallel QEMU/native_sim tests), and then either make the scan
cheaper, for example with one `git grep` over the tree instead of a loop of
greps, or give the test a deliberate `test-group`/timeout override that names
why. Do not raise the default timeout.
