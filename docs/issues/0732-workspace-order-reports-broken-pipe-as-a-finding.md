---
id: 732
title: "`check-workspace-order`'s T2 scenario reports a SIGPIPE from its own harness as \"the provider stopped being discoverable\""
status: open
type: bug
severity: medium
area: build, testing
related: [issue-0726]
---

## Symptom

Seen once in a `check-fast` run, under the parallel gate fan-out:

```
thread 'main' (1035604) panicked at library/std/src/io/stdio.rs:1166:9:
failed printing to stdout: Broken pipe (os error 32)
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
FAIL[T2]: the provider stopped being discoverable
error: recipe `check-workspace-order` failed on line 4602 with exit code 1
```

The panic and the verdict are the same event. A child process died writing to a
closed pipe, and the scenario turned that into a confident, specific claim about
the workspace: that a provider is no longer discoverable. It is not — the gate
passes.

## Why it is worth fixing rather than re-running

This is **issue 0726 one gate over**. 0726 is about `grep -q` conflating a tool
ERROR with a NON-MATCH, and it was found because a forked grep that failed to
start under a 32-way fan-out was reported as a missing force-link anchor. Same
shape here, different tool: the harness cannot distinguish "the provider is
absent" from "the process that would have told me died", and it defaults to the
first.

The direction matters. It fails green→red only under load, which is the
direction that teaches people to re-run a gate instead of believing it — and a
gate nobody believes is worse than no gate.

## Reproduction

Not deterministic. Measured after seeing it once:

* `just check-workspace-order` solo — 3/3 pass
* `just check-fast` — 2/2 pass, zero `Broken pipe` occurrences

So it needs the fan-out plus load. It was observed on a run whose `check-fast`
list had just gained a gate (`check-export-f-closure`, issue 0712), which
changes fan-out composition — that is a plausible trigger for pipe pressure but
was not isolated, and the defect is in the scenario's error handling either way.

## What a fix looks like

The T2 scenario should distinguish its subject failing from its harness failing,
the way `nros_grep_q` does: a tool that did not run exits the check with a
distinct status and a message naming the tool, instead of producing a verdict
about the tree. `scripts/lib/grep-q.sh` records the reasoning; this needs the
same treatment for whatever child writes into that pipe.

Worth checking the sibling scenarios in the same gate at the same time — a fix
that lands only on T2 is the pattern issue 0712 was filed about.
