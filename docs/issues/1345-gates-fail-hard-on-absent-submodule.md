---
id: 1345
title: "Two gates report a hard FAIL for \"I have no tree to look at\", which is
  the DEFAULT state in an agent worktree — so `ci gate` stops at step 2 and four
  later steps are withdrawn, having examined nothing"
status: open
type: bug
area: ci, tooling
severity: medium
found: 2026-09-12
related: [issue-0952, issue-1043, issue-1280, phase-454]
---

## Measured

In a linked agent worktree, `just ci gate` stopped at **step 2 of 6** on two
gates, neither of which had examined the commit under test:

```
check-capability-conditionals: zenoh-pico/include/zenoh-pico/system/common/platform.h
  is missing … The socket-ABI rule cannot be checked without it.
check-xrce-vendored-versions: no vendored tree checked out — nothing verified.
```

Both messages say plainly that **nothing was verified**. Both exit non-zero
anyway.

The cause is not a defect in the submodules: `zenoh-pico`, `micro-cdr` and
`micro-xrce-dds-client` were empty directories, their object stores already
present in `.git/modules`. `git submodule update --init` on the three checked
them out at their recorded pins **with no fetch**, and both gates then passed:

```
check-capability-conditionals: OK (5 platform manifest(s), 5 capability-gated row(s), IP_STACK = 'ip_stack')
check-xrce-vendored-versions: OK — micro-cdr 2.0.2, micro-xrce-dds-client 3.0.1
```

An agent worktree does not init submodules by default, so **"no tree to look at"
is the ordinary condition there, not an exception**.

## The distinction these gates do not make

`check-submodule-pins` already models this correctly and issue 1043 is the
write-up: it has **three** outcomes, not two — FAIL (ancestry measured, not a
fast-forward), **NOT VERIFIED** (no object store here — a reported skip via the
`nros_check_skip` ledger, because no lane checks out all 20 submodules), and OK.

The mechanism is in the tree and other gates use it (`scripts/build/check-skip.sh`;
`check-cxx-compat-shim-facilities.py`, `check-ivc-fsp-compile.sh` among its
consumers). Neither gate here references it:

| gate | `nros_check_skip` references |
| --- | --- |
| `scripts/check-capability-conditionals.py` | **0** |
| `scripts/check-xrce-vendored-versions.py` | **0** |

So each collapses "I could not look" into "I looked and it is wrong", which are
different claims with different remedies.

## Why the cost is larger than two gates

`ci gate` stops at the first failing step. A FAIL at step 2 **withdraws
`check::build`, `check::api-parity`, `test-unit` and `test-lane-contracts`** —
four steps that never ran, over a commit neither failing gate had read.

That is issue 0952's shape, and the lane's own footer warns about it. The
practical effect during phase-454: **five consecutive waves could not obtain a
local `check::build` verdict**, each spent time triaging reds that were
environmental, and each fell back to CI for the answer. Combined with issue 1280
(inherited paths making a worktree build measure the wrong tree), the compile
tier has been effectively unavailable to parallel agent sessions.

A red lane that is red for a reason unrelated to the change also has **no signal
capacity** — the class CLAUDE.md describes: a regression landing in it looks
exactly like yesterday's failure.

## What a fix has to decide

* Route both gates through the existing skip ledger so an absent vendored tree is
  **NOT VERIFIED**, reported, and non-fatal — matching `check-submodule-pins`.
* Decide whether the ledger's report should be loud at the END of the lane rather
  than inline, so a lane that skipped half its gates cannot read as a clean green.
  `check-submodule-pins` has `NROS_SUBMODULE_PINS_STRICT=1` for a lane that really
  does provide every submodule; the same escape hatch applies here, and issue 1043
  records that setting it on a lane providing a subset **re-creates the bug**.
* Sweep rather than fix the two reported sites. The rule is "a gate that cannot
  reach its subject reports NOT VERIFIED", and these two were found by tripping
  over them — issue 0196's shape says there will be more. Enumerate every gate
  that reads a vendored tree.

## Not the same as issue 1280

1280 is *inherited absolute paths make a worktree build the wrong checkout*. This
is *an absent vendored tree is reported as a defect*. They compound — a worktree
hits both — but the fixes are independent, and 1280's fix does not address this.

## Reproduction

In any linked agent worktree with uninitialised submodules, run `just ci gate`.
It stops at step 2 with the two messages above. `git submodule update --init
zenoh-pico micro-cdr micro-xrce-dds-client` (no fetch needed — the object stores
are already in `.git/modules`) makes both pass.

Found during phase-454 W8.
