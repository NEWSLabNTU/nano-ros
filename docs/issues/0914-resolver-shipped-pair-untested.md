---
id: 914
title: "Nothing exercises the SHIPPED resolver + `pyexec` pair, so a resolver that
  cannot evaluate anything passes every check"
status: open
type: tech-debt
area: testing
related: [issue-0897, issue-0400, phase-332]
---

## Problem

`nros-launch-resolve` is TWO artifacts since issue 0897 W3: the binary, and
`libplay_launch_parser_pyexec.so` beside it, which the binary `dlopen`s at
runtime against a discovered CPython. Neither one alone can evaluate a
`$(eval …)` substitution or a `.launch.py`.

No test runs that pair as installed. The end-to-end test that looks like it
does — `pyload/tests/loads_a_real_interpreter.rs` — **builds its own copy of
the Python half** inside the test, deliberately:

> Built by the test itself rather than depended on, because the dependency is
> what this design removes: a normal dep would put `libpython` back in this
> binary's `DT_NEEDED` and the test would pass for the wrong reason.

That reasoning is correct for what that test asserts. The consequence is that
it passes whether or not `just setup-launch-resolve` ships anything.

## What it cost

`host-tests` was red on `main` for the whole interval between the W2b pin and
PR #73, failing every run on
`demo_bringup/launch/multihost.launch.xml`:

```
this build has no Python backend, and a `$(eval …)` substitution needs one:
  $(eval "robot1" in ("robot1", "all"))
```

on hosts that have Python. Two independent defects, either sufficient:

1. nano-ros's `nros-launch-resolve` called `play_launch_parser::parse_launch_file`
   directly, bypassing `ros_launch_resolve::verbs::parse_launch_file` — the one
   place `pyload::install()` runs. Its doc comment claims that is "one place to
   change"; there were two entry points.
2. `setup-launch-resolve` built only the binary, never the Python half, so the
   loader had nothing to find.

Both were fixed in PR #73. Neither would have survived a test of the shipped
pair.

## Why the checks did not catch it

The claim that was verified is `readelf -d` showing no `libpython` in the
binary. That is the **shape** of the artifact, and it remained true the whole
time — it is true of a resolver that loads an interpreter and equally true of
one that cannot. **Removing a link and loading one are two claims, and only
the first was tested.**

This is the archived-0400 family one step further on: that issue's
recommendation was refuted by measuring the artifact, and the fix was then
validated the same way. Measuring an artifact answers "is the link gone", never
"does the capability work".

## Direction

A test that runs the resolver **as `setup-launch-resolve` leaves it** against a
launch file that requires Python, and asserts a model comes out. The fixture
already exists and is the one CI failed on
(`examples/workspaces/rust/src/demo_bringup/launch/multihost.launch.xml`,
`host:=robot1` -> 1 node).

Two properties it must have, or it re-creates the hole:

- It must invoke the INSTALLED binary by path, not link the library. A test
  that builds its own `pyexec` proves the mechanism and not the packaging,
  which is exactly the gap.
- A host with no usable interpreter must SKIP with `nros_tests::skip!`, not
  pass. "No Python here" and "the pair is broken" produce the same parse error,
  and a test that cannot tell them apart is worse than none — it is the
  vacuous-test class `check-no-vacuous-tests` exists for.

Worth pairing with an assertion that both artifacts exist after
`setup-launch-resolve`, since "already built" meaning only the binary is what
let the recipe report success on a tree that could not evaluate anything (also
fixed in #73, also untested).
