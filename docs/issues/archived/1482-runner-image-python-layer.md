---
id: 1482
title: "the runner image is generated from `[prereq.*]` and stops there — the
  `[python.*]` half of the same index never reached it, so a missing module read
  as a host-provisioning gap nobody could fix from inside the container"
status: resolved
type: bug
area: [ci, tooling]
severity: high
found: 2026-09-24
resolved_in: "this commit"
related: [1457, 1481, 1477, 1483, 0368, 0500, 0833]
---

## The governing rule

A maintainer decision, recorded here because it decides where several other
fixes belong:

> **A self-hosted runner is a container. When it lacks something, the fix is the
> IMAGE — never the host environment.** Installing on the host produces a
> machine nobody can account for and that no fresh container reproduces.

Three separate sessions had drifted toward "install X on the runner box", and
each reading was locally reasonable: issue 1457 ends "the remaining work is on
the host"; issue 1477 ends "installing `ros-humble-rmw-zenoh-cpp` there needs
root on a self-hosted runner, which no agent has". Both are the host answer to a
container question.

## What was actually wrong

`scripts/ci/runner-container.sh` generates its Dockerfile from
`nros-sdk-index.toml` and says so in its own error text — writing the package
list by hand "would create the second source of truth this script exists to
avoid". But it resolved only `[prereq.*]`, through
`scripts/sdk/prereq-packages.py`. The `[python.*]` layer — the modules the
ambient interpreter must be able to import — had no path into the image at all.

So the image carried no `catkin_pkg`, `em`, `lark`, `yaml` or `tomli`, and
tier-2 nightly died eleven minutes into a Zephyr build:

```
File ".../rosidl_adapter/rosidl_adapter/cli.py", line 19, in <module>
    from catkin_pkg.package import package_exists_at
ModuleNotFoundError: No module named 'catkin_pkg'
```

Half an index reaching the image is worse than none of it, because the half that
does reach it makes the arrangement look complete.

## Why the image is the only place it could come from

Not a preference — there is exactly one producer, and the other two candidates
are closed by their own rules:

* the **running container** is `--cap-drop ALL --security-opt
  no-new-privileges` with a non-root user, so `sudo apt` inside a job cannot
  work even where the package name is right;
* **`runner-provision.sh`** never sudoes and never installs a system package, by
  the convention phase-327 W2 / issue 0368 F1 established (sudo-less installers
  run first; the system-package step only *prints*);
* **the host** is not a producer at all: what is installed there is in no file,
  is invisible to the image, and no fresh container has it.

## This is not "the runner needs ROS"

A ROS-less runner is the design. The tier-2 job runs on `[self-hosted, linux,
nros-qemu, nros-sdk-zephyr, nros-big]` — no `nros-ros2` — and
`runner-provision.sh` has an `nros-ros2` label it deliberately does not
provision (`_provision_ros2_notice`). Issue 0368 / phase-327 created the
`[python.*]` layer *precisely* so the cyclone msg→IDL road works without a ROS
install. Nothing about ROS was missing; the layer was.

## Which entries — DERIVED, not listed

`scripts/sdk/python-packages.py` takes the `[python.*]` entries with **no
`check = { cmd = … }`**. That is not a new convention: the index's own
`[python.*]` header says an entry with no `check` is probed by importing its
`module` in the host's `python3`, so such an entry is a claim about the AMBIENT
interpreter — the one cmake, ninja and `msg_to_cyclone_idl.py` invoke — which in
a container is the image's job and nothing else's.

An entry WITH a command is an executable one provisioning verb installs into a
place the container already persists, and baking a second copy is issue 0500's
shape:

| entry | why not the image |
| --- | --- |
| `west` | the in-repo Zephyr venv (`scripts/build/zephyr-python.sh`, group `west` in `check-python-deps.py`), inside the `src` volume. Worse than duplication: `runner-doctor.sh` probes `command -v west`, so an image copy would make `nros-sdk-zephyr` true by CONSTRUCTION rather than by provisioning |
| `clang-format` | `just setup-clang-format` puts the wheel's standalone binary under `build/clang-format/bin`; the index's own `apt_refused` says "no venv, nothing user-wide" |
| `colcon` | the `rmw_zenoh` overlay build, which needs a ROS install this runner does not have and does not claim |

Derived rather than listed, so a new `[python.*]` entry joins or stays out by
what it declares — there is no list in the script to forget to update. Today it
resolves to `catkin-pkg empy lark pyyaml tomli`.

## apt vs pip — measured IN the image, and the ROS repo is not needed

Issue 1481 gave `[python.*]` an apt position (`apt = [..]` xor `apt_refused`)
and a resolver that asks apt for a candidate before preferring it. The host that
GENERATES the Dockerfile is the wrong host to ask. Measured, same package, three
answers:

| host | `apt-cache policy python3-catkin-pkg` |
| --- | --- |
| this workstation (ROS 2 apt repo) | candidate `1.1.0-101` |
| `ubuntu:22.04`, no ROS repo | candidate `0.4.24-2` (jammy universe) |
| a host with no apt | nothing |

Encoding any of those at generation time makes the image's contents depend on
who ran the generator. So the split runs in two halves, each where its facts
are: `--emit json` reads the index (host-independent declarations), and
`--resolve` asks THAT image's apt at build time, by the same rule as
`python_provider::resolve`.

**The answer, measured in `ubuntu:22.04`, is apt for all five and pip for
none** — so the question "should the image add the ROS 2 apt repo?" does not
arise, and the answer is no regardless: it would widen the image's trust base to
a third-party archive for one pure-python package Ubuntu already carries, on a
runner that deliberately claims no ROS label. Had any entry fallen to pip, pip
*inside the image* would have been correct anyway — there is no ROS install in
there for a `~/.local`-style copy to shadow, which is the hazard 1481 is about.

One correction the measurement forced: `[python.catkin-pkg]`'s comment, and
`python_provider.rs`'s module doc, both said `python3-catkin-pkg` comes only
from packages.ros.org/ros2. It does not — jammy universe carries `0.4.24-2`, and
`from catkin_pkg.package import package_exists_at`, the import
`rosidl_adapter.cli` actually makes, resolves under it. Both comments are fixed.
That is the case for measuring rather than tabulating, made against the very
table that was written to explain why measuring was necessary.

## A second, quieter defect this uncovered

`check-ci-image-apt-packages` globs `ci/docker/*/Dockerfile` and requires every
match to `COPY ci/docker/apt-packages.txt`. `runner-container.sh` writes its
build context to `ci/docker/runner/`, which `.gitignore` carries — so **anyone
who had ever run that script had a red `just check fast`**, about a build
artifact, naming a shared list that image has no business consuming (it is
`FROM ubuntu:22.04`, not `ros:humble-ros-base`, and its packages come from the
index).

The gate now skips Dockerfiles git does not TRACK. Trackedness, not the
directory's name: a hand-maintained image is committed, build output is not, and
a future generated context is covered without editing the gate. Two self-test
cases hold it — the untracked file is ignored, the same file committed still
fails — because a skip with no negative control is an excuse, not a rule.

## Negative control

On the pre-fix tree, generate the Dockerfile and look for the layer:

```
$ ./scripts/ci/runner-container.sh nros-qemu,nros-sdk-zephyr,nros-big --check
$ grep -nE 'catkin|empy|lark|yaml|tomli|pip3|python-layer' ci/docker/runner/Dockerfile
(none — grep found nothing)
```

Post-fix, same command:

```
# Keys: catkin-pkg empy lark pyyaml tomli
COPY nros-python-layer.json python-packages.py index_packages.py /opt/nros-python/
RUN apt-get update \
    && python3 /opt/nros-python/python-packages.py \
         --resolve /opt/nros-python/nros-python-layer.json --emit plan \
    ...
```

And the layer built in the base image it is written for (`ubuntu:22.04`, the
generated text sliced out of the real Dockerfile):

```
nros [python.*] layer — release jammy
  catkin-pkg     apt python3-catkin-pkg                 (apt has a candidate for every declared package)
  empy           apt python3-empy                       (apt has a candidate for every declared package)
  lark           apt python3-lark                       (apt has a candidate for every declared package)
  pyyaml         apt python3-yaml                       (apt has a candidate for every declared package)
  tomli          apt python3-tomli                      (apt has a candidate for every declared package)
python-packages: verified catkin_pkg em lark yaml tomli import in /usr/bin/python3
```

The FULL image was not built here — it fetches a ~200 MB runner tarball and the
whole `[prereq.*]` closure, which is not something to spend a workstation's disk
on to prove a layer that was built on its own.

The gate's own control:

```
$ python3 scripts/check-ci-image-apt-packages.py   # with ci/docker/runner/ present
check-ci-image-apt-packages: FAILED                # before
check-ci-image-apt-packages: OK (18 shared package(s), 2 image(s): ci-base, zephyr-ros)   # after
$ python3 scripts/check-ci-image-apt-packages.py --self-test
check-ci-image-apt-packages self-test: OK (12 cases)
```

## Where the practice is written down

Because the reading, not the packages, is what kept coming back:

* `docs/development/multi-agent-ci-workflow.md` — "A missing dependency on a
  self-hosted runner is fixed in the IMAGE", beside the container-security
  argument it follows from, with the rebuild/restart/verify loop;
* `CLAUDE.md` pitfall index — one line, pointing there;
* the headers of `runner-container.sh`, `runner-provision.sh` (where "it never
  installs a system package" now says what does) and `runner-doctor.sh` (where a
  `[MISSING]` line gets fixed).

## What this does NOT fix

The zenoh half. Tier-2's zenoh interop cells still `[SKIPPED:capability]`
because the runner has no `rmw_zenohd`, and that is a bigger decision than this
one: a router comes from a ROS install (RFC-0075 ships none), so making it true
means either an `nros-ros2`-labelled runner image with the ROS apt repo in it, or
accepting the skip. Issue 1477 records the symptom and is corrected to say the
decision is about the image, not about who has root.

## Follow-on — issue 1483

A maintainer rule arrived while this was in flight: **the index must not carry
transitive dependencies.** Measured against it, two of the five entries this
image now installs — `empy` and `lark` — are upstream rosidl's alone and belong
with `[source.rosidl]`, reached through the pinned `nros-rosdep-snapshot.toml`
rather than hand-listed here. (`pyyaml` was relayed as a third; it is not —
eleven of our own scripts import `yaml`, several of them `check-fast` gates.)

Nothing here changes. The image layer is DERIVED from `[python.*]`, so when
those two leave, the layer follows with no edit to `runner-container.sh` — which
is the property that let this land first. The one direction that is not safe is
removing them before the replacement exists: this runner is `FROM ubuntu:22.04`
with no ROS repo, i.e. exactly the ROS-less host that layer was created for, so
that order re-opens 1457 here. Issue 1483 carries the measurement and says so.
