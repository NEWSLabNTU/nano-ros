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

## Still reproducing six days later, and a third call site

Re-measured 2026-09-19 from `live-peer regression` run **35420818887**
(schedule, 04:15), which is the same two-job/two-cause shape this issue
recorded on 2026-09-13:

| job | id | cause |
| --- | --- | --- |
| `rows whose board IS this runner` | 105838049568 | `No space left on device : '…/_diag/Worker_20260919-041559-utc.log'` — [[issue-1353]], read from the check-run annotations because the log itself is 48 833 lines with no diagnostic in it |
| `rows whose board is NOT this runner` | 105838225959 | this issue |

So the lane has now been red on this cause for every scheduled run from
2026-09-11 to 2026-09-19 inclusive. Nothing about the image changed in that
window, which is what the issue predicts; it is recorded here because a lane red
every night has no signal capacity, and the only way to keep it from absorbing a
second fault is to re-read the text each time.

**A third script dies on the same chain.** The issue names
`scripts/build/fixtures-manifest.py` and `scripts/sdk/prereq-packages.py`. This
run adds `scripts/check-interop-verdicts.py`, two seconds after the fixture
build gives up:

```
File "/__w/nano-ros/nano-ros/scripts/check-interop-verdicts.py", line 100, in <module>
    import tomllib
ModuleNotFoundError: No module named 'tomllib'
…
  File "/__w/nano-ros/nano-ros/scripts/check-interop-verdicts.py", line 102, in <module>
    import tomli as tomllib  # type: ignore
ModuleNotFoundError: No module named 'tomli'
error: recipe `interop-verdicts` failed on line 877 with exit code 1
```

That does not change the diagnosis, but it does change the shape of option 2
above: the sweep is not "the scripts the fixture build happens to reach", it is
every one of the 39 sites, because the job walks through them one recipe at a
time and each dies on its own import. It also means the job would not reach its
cells even if the fixture build were fixed on its own.

## What landed (phase-466, 2026-09-23)

**Option 1, and it is now one decision with [[issue-1359]] rather than two.** The
issue offered "the image provides a parser" or "the scripts stop needing one" and
asked for the choice to be made and bound by a gate. The choice is option 1, for a
reason the issue could not see on its own: `unzip` was missing from the same image,
in the same way, for the same reason — two hand-written apt lists that had to agree
with nothing making them agree. Fixing a TOML parser into one list would have left
the mechanism intact and the next drift waiting.

- **`ci/docker/apt-packages.txt`** is the one apt closure every CI image installs;
  `python3-tomli` is in it, beside `unzip`. Both Dockerfiles `COPY` it and pipe it
  through `xargs apt-get install`, so an image cannot have one and not the other.
- The requirement is **DERIVED, not asserted**: `check-ci-image-apt-packages`
  greps the tree for the `import tomllib` -> `import tomli` chain and demands a
  parser package in the shared list for as long as any script uses it. Measured on
  the real tree: 40 sites today (the issue counted 39). Remove the last one and the
  gate stops asking, which is what makes it a rule about the tree rather than a
  constant.
- The gate itself parses `nros-sdk-index.toml` with regex on purpose, and says so:
  it has to run on a host with no TOML parser, because that is the state it exists
  to describe. `scripts/dev/clang-format.sh` reads the same file the same way.

**The 39-site sweep (option 2) is deliberately NOT done, and this records why.** It
would only have helped if SOME parser were present, and the issue itself notes the
image has no `toml` either — so a widest-first chain would have added a third arm
that also fails. With the image providing `tomli`, all 40 sites work unchanged and
the chain they already carry is correct. If a shared `scripts/lib/` loader is still
wanted, it is now a readability change and not a fix.

## What is NOT done, and why this stays open

The image is published only by `images.yml` on a push to `main`, and the consumers
moved to `humble-sdk0.17.4-r5`, which does not exist in the registry until that
workflow has run. Acceptance is unchanged: the `rows whose board is NOT this
runner` job reaching its cells.
