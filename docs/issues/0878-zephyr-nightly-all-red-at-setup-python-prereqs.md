---
id: 878
title: "Every Zephyr nightly cell fails at `just zephyr setup` on missing Python
  prereqs — 21 of 21 red, so the lane cannot report a regression"
status: open
type: bug
area: ci
related: [issue-0876, issue-0873, phase-395]
---

## Problem

Every Zephyr cell in the `nightly` workflow is red, and all of them fail in the
same place — the `Set up Zephyr <line> workspace` step, before any build:

```
[zephyr-build] `west build` for any Zephyr board (the subset of requirements-base.txt our lanes import)
    import elftools     (pip: pyelftools)
    import pykwalify    (pip: pykwalify)

[ERROR] Python prerequisites missing — see the report above.
error: recipe `setup` failed with exit code 1
```

Both lines (3.7 and 4.4), all languages, all roles:

```
zephyr ci-both (3.7 + 4.4)        failure
zephyr copy-out check (4.4)       failure
zephyr 3.7 / {c,cpp,rust}/*       failure   (11 cells)
zephyr 4.4 / {c,cpp,rust}/*       failure   (10 cells)
```

The container is `ghcr.io/newslabntu/nano-ros-zephyr-ci:humble-sdk0.17.4-r2`.
Its Dockerfile installs `python3 python3-pip python3-venv` but never creates the
venv or installs `west` / `pyelftools` / `pykwalify` / `PyYAML` / `packaging`
into it. `scripts/zephyr/*` deliberately does not install them — the script says
so, and the reasoning is sound (choosing between a distro package, `pip --user`
and a venv is a decision about the host's interpreter, and PEP 668 hosts refuse
`pip --user` outright). But that reasoning is about a *developer's* host. The CI
image is ours, and the decision it defers to the user has no user to make it.

## Why this matters more than one red lane

A lane that is uniformly red has **no signal capacity**. A new regression
entering it is indistinguishable from the standing failure, so the lane keeps
reporting exactly what it reported yesterday and the regression rides in
unnoticed.

That is not hypothetical here — it is how [issue 0876](archived/0876-zephyr-talker-heap-pool-zero-breaks-native-sim.md)
survived. A conf change made the C talker unbuildable on native_sim, the nightly
has a `zephyr 3.7 / c/talker` cell that would have caught it, and that cell
reported `failure` both before and after the change. It was found by hand a day
later, while measuring something unrelated.

Same shape as [issue 0873](0873-nightly-offline-lockfile-cold-cache.md) — three
platform cells red on a cold cargo cache — and the same shape `queue-triage`
answers for the merge queue: **is this red mine, or is it red for everyone?**
The merge queue has a tool for that question. Nightly does not.

## Fix

Bake the prereqs into `ci/docker/zephyr-ros/Dockerfile`, where the image's own
interpreter is not a user decision. The venv `activate.sh` already looks for is
`scripts/zephyr/.venv`, and the script prints the exact command:

```
python3 -m venv --system-site-packages scripts/zephyr/.venv
scripts/zephyr/.venv/bin/pip install west pyelftools PyYAML pykwalify packaging
```

Pin the versions, as `ci-base` now does for `uv` and `just` — an unpinned `pip
install` in an image build is the floating half of the drift axis that
`docs/development/ci-image-provisioning.md` describes. Bump the image tag with
it (`-r3`), since the tag is what makes a bump non-silent.

Note this is the SIBLING image, not `ci-base`. They inherit nothing from each
other (issue 0866), so a fix in one does not reach the other.

## The larger point, not fixed here

Two lanes have now been fully red for reasons unrelated to the code they test,
and in both cases the standing red is what hid a real defect. Worth its own
work item:

- **A lane that produces no verdict should say so louder than a lane that fails
  a test.** `setup` failing and `test` failing are the same colour today and
  should not be. phase-395 built exactly this distinction for the merge queue
  (`queue-triage`'s INFRA vs MINE); nightly has no equivalent.
- **A cell that has been red for N consecutive runs is not reporting.** Nothing
  currently tracks that, so "still red" and "newly red" look identical in the
  run list.
