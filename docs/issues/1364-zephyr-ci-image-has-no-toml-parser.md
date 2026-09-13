---
id: 1364
title: "The zephyr CI image is Python 3.10 and ships no TOML parser, so every repo
  script that reads a manifest dies in it — and the gate that exists for this class
  reads two dependency groups neither parser is in"
status: open
type: bug
area: [ci, tooling]
severity: medium
related: [issue-0878, issue-1353, issue-1019, issue-1226]
---

## What happens

`live-peer regression`, run **34737486682** (schedule, 2026-09-13T04:16), job
**103671448349** (`rows whose board is NOT this runner`), step
`Build the fixtures those rows resolve`:

```
File "/__w/nano-ros/nano-ros/scripts/build/fixtures-manifest.py", line 32, in <module>
    import tomllib  # Python 3.11+
ModuleNotFoundError: No module named 'tomllib'
…
File "/__w/nano-ros/nano-ros/scripts/build/fixtures-manifest.py", line 34, in <module>
    import tomli as tomllib
ModuleNotFoundError: No module named 'tomli'
```

and, earlier in the same job, the same pair from
`scripts/sdk/prereq-packages.py:40,42`.

That job runs in `ghcr.io/newslabntu/nano-ros-zephyr-ci:humble-sdk0.17.4-r4`,
whose base is `ros:humble-ros-base` — Ubuntu jammy, system Python **3.10**, which
has no `tomllib`. The image's `pip3 install` block installs
`west pyelftools PyYAML pykwalify packaging jsonschema` and nothing else, and no
`apt` line installs `python3-tomli` or `python3-toml`. So no TOML parser exists in
that image at all, and every repo script that reads a manifest fails on import.

## What the failure text costs

The step that dies is `Build the fixtures those rows resolve`, and the visible
consequence is `zephyr-fixture-leaves: no records matched filter:` — a message
about the FILTER, in a probe whose own stderr carries the import error. The lane
has been red on 2026-09-11, -12 and -13, and was being attributed to issue 1353
(the runner's disk), which is the correct cause for the OTHER failing job in the
same run — 103671313120 fails with
`No space left on device : '…/_diag/Worker_20260913-041609-utc.log'` — and is not
this one. Two failing jobs, two causes, one run.

## Why the existing gate does not see it

`check-ci-image-python-deps` was built for exactly this class (issue 0878: the
image not installing what `check-python-deps.py` demands). Its scope is
`GROUPS["west"] + GROUPS["zephyr-build"]`, and a TOML parser is in neither group,
because those groups answer "will `west build` work here". So the gate is green
while the image cannot run the repo's own build scripts — the reach of a gate
being narrower than the rule it enforces, the shape issue 1226 records.

## The repo-wide half

**39 Python files** in this tree open with the two-step chain and no third arm:

```sh
grep -rln --include='*.py' -E "^\s*import (tomllib|tomli)\b" . | grep -v third-party
```

Exactly one of them, `scripts/check-feature-contract.py`, carries a widest-first
chain ending in the older third-party `toml` — added by PR #1019 for this same
failure in a different lane. That fix landed at the reported site and added a
second idiom rather than one shared helper, which is the pattern CLAUDE.md names;
`scripts/lib/` has no TOML loader for the 39 to share. Whether a third arm would
even have helped here is unmeasured: this image has no `toml` either.

## What this is NOT

- **Not issue 1353.** That is the hosted runner's disk, and it is the cause of the
  *other* failing job in this same run.
- **Not issue 0878.** That was the west/zephyr-build modules, which the image does
  now install; this is a module no group lists.
- **Not a script bug on the merits.** `fixtures-manifest.py` and
  `prereq-packages.py` read TOML because their input is TOML, and on any Python
  3.11 host they work. The disagreement is between the image and the interpreter
  it ships.

## What would close it

A decision between the two defensible sites, then the gate that binds it:

1. **The image provides a parser** — add a pinned `tomli` to the zephyr image's
   pip block, and add it to a `check-python-deps.py` group so
   `check-ci-image-python-deps` can see it. The group would be a new one (it is
   not a `west build` dependency), which is the honest shape.
2. **The scripts stop needing one** — one shared `scripts/lib/` loader with the
   widest-first chain, adopted by all 39 sites in one sweep, so there is a single
   spelling to fix next time. This only helps if some parser is present; measure
   that in the image before choosing it.

Acceptance is the `rows whose board is NOT this runner` job reaching its cells, and
a gate that fails if the image loses the parser again.
