---
id: 397
title: a failing nros CLI made check-model-dims report every dim of every model as LOST
status: resolved  # fixed 2026-08-03
type: bug
area: tooling
related: [0380, 0395, 0196]
---

## Problem

`scripts/check-model-dims.sh` read each model's dims with

```sh
done < <("$nros_bin" ws model-dims "$model" 2>/dev/null || true)
```

so ANY non-zero exit produced zero dims for that model. When the failure is the
CLI itself rather than one file — the common case, a stale-CLI refusal after a
rebase moves `packages/cli/` — every model reads empty and the gate reports the
whole baseline as lost:

```
[FAIL] committed SystemModel(s) LOST execution dims (issue 0380):
       …/freertos_system_model.yaml	high.freertos.priority
       … 118 lines …

       These are hand-authored and the resolver cannot put them back.
       A regeneration almost certainly stripped them — restore from git
       history rather than re-resolving.
```

Every one of those dims was present in the committed YAML. Hit for real while
closing issue 0395, immediately after a rebase that pulled a CLI change.

Two ways that hurts:

1. **The advice is actively wrong for the actual fault.** The report tells you
   to restore generated files from git history; the remedy is `just setup-cli`,
   which the CLI's own error states and `2>/dev/null` discarded.
2. **`--write` shares the loop.** Re-recording while the CLI is broken bakes the
   empty reading in and destroys the record the gate exists to keep — the exact
   loss it watches for, committed by the tool that watches.

## Fix

A per-model failure is fatal and names the model plus the CLI's stderr; the
comparison and the re-record are both refused. Verified by pointing `$NROS_CLI`
at a stub that exits 1: exit 1, the CLI's message reaches the user, and
`--write` leaves the baseline untouched.

One subtlety worth keeping: the first version set a shell VARIABLE from inside
the loop, which is the left side of a pipeline and therefore a subshell — the
flag never reached the parent and the gate passed a stub CLI cleanly. It is a
marker file now. A gate whose failure path has not been watched to fire is a
guess; this one was wrong on the first try.

## Not the same class

`check-board-manifest-drift.sh` also does `nros_cli_bin 2>/dev/null || true`,
but it SKIPS on absence with a rebuild hint and probes for the verb it needs. It
reports nothing it cannot substantiate, which is the opposite behavior.
