---
id: 1043
title: "`check-submodule-pins` fails CLOSED on any submodule CI does not
  initialise, so a whole class of pin bump could never pass the required lane"
status: open
type: bug
area: ci, tooling
severity: high
found: 2026-09-04
related: [issue-0996, issue-1034, phase-395]
---

## What happened

The nuttx pin bump in issue 1034 passed `just check fast` locally (197 gates OK)
and failed the required `CI` context with:

    ===== FAIL (submodule-pins, rc=1, 459ms) =====
    submodule-pins: CANNOT VERIFY third-party/nuttx/nuttx
        the pin moved c3fa5dfb0673 -> aea00d736d69 but the submodule is not
        initialised here, so its history cannot be read.
        Run: git submodule update --init third-party/nuttx/nuttx

The remedy it prints is addressed to a developer with a checkout. In CI nobody
can act on it: the `check` job initialises exactly one submodule
(`packages/cli/third-party/play_launch`), plus the `-sys` sources `nros setup`
provisions on the compile tier. `third-party/nuttx/nuttx` is in neither set, so
the objects the gate needs were never going to exist.

**So the failure is unconditional.** Re-running, rebasing, or pushing the
submodule commit first — which was already done — changes nothing. Any pin bump
to any submodule outside that small set is unmergeable through the required
lane, and the gate reports it as if it were the author's mistake.

## Why the existing machinery did not cover it

`gate.yml` already has a step for the SHALLOW version of this trap ("Fetch
submodule history for check-submodule-pins"), whose own comment says the gate
"cannot fetch (check-fast is network-free), so the workflow provides". That step
is guarded by `[ -e "$path/.git" ] || continue` — it deepens submodules that
exist and skips the ones that do not, which is exactly the case that cannot
recover on its own.

The class is the one CLAUDE.md names for `check-lane-contracts`: a gate in an
affordability tier may only resolve artifacts the job itself provides.
`check-lane-contracts` enforces that for fixture STAMPS; a submodule object
store is the same kind of dependency and is not covered.

## Fixed here (the workflow half)

A step ahead of the existing one initialises, commits-only, any submodule whose
pin differs between the base and the head:

    git submodule update --init --filter=tree:0 "$path"

`--filter=tree:0` because `merge-base --is-ancestor` needs commits and nothing
else. NOT `--depth`: the neighbouring comment already records that a shallow
fetch grafts the commit as a root with no parent links, so the ancestry check
then reports `DIVERGED` on a clean fast-forward. Only pins that actually moved
are touched, so a PR that moves none pays nothing — and a pin that moved is
precisely the one the gate is about to need.

## Still open

* **The gate's own message is wrong in CI.** It names a command the runner
  cannot usefully run and reads as author error. It should distinguish "you have
  no checkout of this submodule" from "this lane never provides one", the way
  the shallow arm below it already distinguishes its two causes.
* **`check-lane-contracts` does not cover submodule object stores**, only
  fixture stamps. The rule it encodes is the right one; its coverage is narrower
  than the rule, which is the issue-0196 shape.
* Nothing tests this. The failure was found by a pin bump, and the next class
  member — a submodule that is initialised but whose *history* the workflow's
  deepening step cannot reach — would be found the same way.
