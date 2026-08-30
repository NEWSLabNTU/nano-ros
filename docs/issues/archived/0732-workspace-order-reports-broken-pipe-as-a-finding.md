---
id: 732
title: "`check-workspace-order`'s T2 scenario reports a SIGPIPE from its own harness as \"the provider stopped being discoverable\""
status: resolved
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

* `just check workspace-order` solo — 3/3 pass
* `just check fast` — 2/2 pass, zero `Broken pipe` occurrences

So it needs the fan-out plus load. It was observed on a run whose `check-fast`
list had just gained a gate (`check-export-f-closure`, issue 0712), which
changes fan-out composition — that is a plausible trigger for pipe pressure but
was not isolated, and the defect is in the scenario's error handling either way.

## Superseded by the resolution below

### What a fix looks like

The T2 scenario should distinguish its subject failing from its harness failing,
the way `nros_grep_q` does: a tool that did not run exits the check with a
distinct status and a message naming the tool, instead of producing a verdict
about the tree. `scripts/lib/grep-q.sh` records the reasoning; this needs the
same treatment for whatever child writes into that pipe.

Worth checking the sibling scenarios in the same gate at the same time — a fix
that lands only on T2 is the pattern issue 0712 was filed about.

## Root cause (2026-08-20) — proven, not inferred

```bash
"$NROS" ws providers --workspace "$WS2" … --kind rmw |
    grep -q "acme" || bad T2 "the provider stopped being discoverable"
```

`grep -q` exits at its FIRST match, closing the pipe while `nros` is still
writing. `nros` gets EPIPE; Rust ignores SIGPIPE, so `println!` PANICS
("failed printing to stdout: Broken pipe"). The script runs under
`set -o pipefail`, which takes the rightmost non-zero status — so the pipeline
is non-zero **even though grep matched**, and `|| bad T2` fires.

Demonstrated standalone rather than argued:

```
$ bash -c 'set -uo pipefail; seq 1 5000000 | sed "s/^/acme /" | grep -q acme; echo "rc=$?"'
rc=141
```

141 is SIGPIPE. The producer has to still be writing when grep exits, which is
why it only ever failed under the parallel gate fan-out.

## Fix

Capture, then test. That removes the pipe entirely, and it separates the two
verdicts the pipeline had conflated:

```bash
if ! PROVIDERS="$("$NROS" ws providers … 2>&1)"; then
    bad T2 "ws providers errored: $PROVIDERS"
elif ! grep -q "acme" <<<"$PROVIDERS"; then
    bad T2 "the provider stopped being discoverable"
fi
```

Both branches falsified against the real gate:

| injected | reported |
| --- | --- |
| pattern that cannot match | `FAIL[T2]: the provider stopped being discoverable` |
| `--bogus-flag` on the CLI | `FAIL[T2]: ws providers errored: error: unexpected argument '--bogus-flag' found` |

## The class, and why the site alone was not enough

**This is the second occurrence.** `check-archive-lang-items.sh:93` already
carried the lesson from an earlier round:

> NOT `| grep -q`: with `set -o pipefail`, grep's early exit gives `nm` SIGPIPE
> and the pipeline reports FAILURE on a match — which inverted an earlier
> revision of this gate silently.

That fix stayed a local comment instead of becoming a checkable rule, so the
next author reached for the same spelling. CLAUDE.md's rule is to fix the class
and prove the sweep.

**And #0726's gate could not have caught this one, because it was not looking.**
`check-grep-q-error-conflation.py` scanned `scripts`, `just`, `justfile` only —
`workspace_order_gate.sh` lives in `packages/testing/nros-tests/tests/` and sat
in no baseline at all. Gate scripts that live beside the tests they guard are
checkers too, and "a checker must not report a tool failure as a finding" is no
less true of them. That is issue 0196's shape: a gate narrower than the rule it
enforces.

So the gate's scope now includes `packages/` and `tools/` (excluding
`third-party/` and `generated/`). The sweep made **39 pre-existing sites across
11 files** visible; they are baselined, per that gate's documented ratchet
design — the win is that a NEW one in any of them now fails. Verified by adding
one to a newly-visible file (`4 -> 5`, reported) and removing it again.

Baseline: 133 sites / 74 files → 172 / 85.

## Still open in the class

The baselined sites are `grep -q` CONDITIONALS (#0726's error-vs-non-match
conflation). The *pipeline* variant this issue is about — a program piped into
`grep -q` under `pipefail` — is a distinct failure mode that no gate detects
yet; it is invisible to a per-file count because the count does not care what is
on the left of the pipe. A checker for it would look for `<command> | grep -q`
where the producer is a program rather than a shell builtin. Not written here:
the two known instances are fixed, and a third occurrence is the evidence that
would justify the gate.
