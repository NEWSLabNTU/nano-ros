---
id: 1484
title: "A `[python.*]` entry never said whose dependency it was, so the census
  that answers it kept being re-derived — and twice it was re-derived wrong"
status: resolved
type: tech-debt
area: [tooling, orchestration]
severity: medium
found: 2026-09-24
resolved: 2026-09-25
related: [1483, 1482, 1481, 1457, 0368]
---

## The rule this serves

A maintainer decision, arriving while 1482 was in flight and restated in 1483:

> Our index must not carry transitive dependencies. Where a package is a
> transitive dep of a direct apt ROS dependency, install the apt ROS package and
> let apt resolve it.

Issue 1483 states the debt and what would close it. This one is about the
property that made the debt undetectable: **`[python.*]` had no field that says
whose dependency an entry is**, so the only way to answer it was a fresh census
of the tree — and a census run by hand is a measurement that decays.

Measured cost: the same census was taken three times in one day by three
readers, and `pyyaml` came back "pure transitive, nothing here imports it"
twice. Eleven of our own scripts import it, several of them gates on the
`check-fast` line. Acting on either wrong reading would have removed the entry
that lets a clean host run the lane this repo tells everyone to run before every
push.

## MEASURED (2026-09-24, `ast`-parsed over tracked `*.py`, `third-party/` and `docs/` excluded)

| entry | module | direct importers here | whose |
| --- | --- | --- | --- |
| `tomli` | `tomli` | 42 | ours |
| `pyyaml` | `yaml` | 11 | ours |
| `catkin-pkg` | `catkin_pkg` | 1 (`colcon_nano_ros/workspace_bindgen.py`) | ours |
| `west` | `west` | 1 | ours (tool-probed) |
| `colcon`, `clang-format` | — | 0 | ours (tool-probed executables) |
| `empy` | `em` | **0** | upstream rosidl's |
| `lark` | `lark` | **0** | upstream rosidl's |

Each entry's `why` read the same way regardless of which column it belonged in,
and two of them credited only rosidl for a package eleven of our scripts import.

## The drift this leaves room for, concretely

`[python.lark]` declared `apt = ["python3-lark"]`. Upstream's rosdep key for the
same dependency is **`python3-lark-parser`**, and the mapping between them is
data ros/rosdistro publishes and `nros-rosdep-snapshot.toml` vendors at a pinned
ref. We were hand-maintaining one edge of a dependency graph whose owner
publishes it — and a hand-copy goes wrong in the direction that reads as
correct.

## Why the two upstream entries are NOT simply removed

The rule's remedy is "install the apt ROS package and let apt resolve it", and
on a host with the ROS apt repo it works exactly as stated — measured:

```
apt-cache depends ros-humble-rosidl-adapter   Depends: python3-empy
apt-cache depends ros-humble-rosidl-parser    Depends: python3-lark
```

That host never reaches the vendored clone at all: the adapter lands in
`/opt/ros/humble/lib/rosidl_adapter`, which is rung 2 of
`scripts/cyclonedds/msg_to_cyclone_idl.py`'s ladder, so rung 3 is never opened.

But `[source.rosidl]` exists for the host with **no ROS apt repo** (issue 0368 /
phase-327), where there is no `ros-humble-rosidl-adapter` to install and nothing
for apt to resolve behind. The self-hosted runner container is exactly that
host: `FROM ubuntu:22.04`, no ROS repo, deliberately no `nros-ros2` label — and
issue 1482 made its python layer DERIVED from the `[python.*]` entries with no
`check.cmd`, so deleting the two entries shrinks that image with **no edit to
`runner-container.sh` and no lane watching**.

MEASURED in `ubuntu:22.04`, the runner's own base, with the pinned rosidl clone
and `python3-catkin-pkg` + `python3-yaml` installed:

* **without** `python3-empy` / `python3-lark` — i.e. the index minus the two
  entries — `msg_to_cyclone_idl.py` exits 1 with
  `its python deps are missing: empy==3.3.4, lark`;
* **with** them, apt-installed (`python3-empy 3.3.4-2` satisfies our pin,
  `python3-lark 1.1.1-1`), it emits `Probe.idl` and exits 0.

So removing them before something else supplies that image re-opens issue 1457
on the tier-2 runner. 1483 tracks the move; the image derivation has to follow
it in the same change.

## And upstream's `package.xml` is not, by itself, the replacement

1483 proposes deriving the list from upstream's own `package.xml` through the
rosdep snapshot. The dependency NAMES do come from there — but measured at the
pinned `humble-5621b26`, the manifests declare:

```
rosidl_adapter  <exec_depend>python3-empy</exec_depend>
rosidl_parser   <exec_depend>python3-lark-parser</exec_depend>
rosidl_cli      <exec_depend>python3-argcomplete</exec_depend>
                <exec_depend>python3-importlib-metadata</exec_depend>
```

and **nothing declares `python3-catkin-pkg` or `python3-yaml`**, though
`rosidl_adapter/rosidl_adapter/cli.py` imports both on lines 19–20. A derivation
that trusted the manifest would under-declare exactly the module whose absence
killed tier-2 nightly (1457). Upstream's manifest is authoritative for the names
it states and silent about two it needs; that asymmetry is a fact about the
remedy, not an argument against it.

## What landed

* `[python.*]` entries carry `rosdep = "<upstream key>"` where the dependency is
  not ours. The KEY is upstream's spelling; the apt NAME is the pinned
  snapshot's answer to it, so `python3-lark-parser` → `python3-lark` is no
  longer a string we maintain.
* `check-python-entry-provenance` (fast line) runs the census every time: an
  entry with no importer here and no `rosdep` key is refused, and a `rosdep` key
  must resolve, through `nros-rosdep-snapshot.toml`, to exactly the `apt` names
  declared. Both halves have negative controls in `--self-test`.
* `SdkIndex::validate` refuses `rosdep` beside `apt_refused` (two apt positions)
  and `rosdep` with no `apt` (a key resolving to nothing), preserving 1481's
  "state your apt position" invariant rather than adding a third silence.
* The `why` fields say whose each dependency is and name the importers.
* `[source.rosidl]`'s comment states what the ROS-less path needs, whose each
  one is, why the list is not derivable from upstream's manifest alone, and that
  a ROS-repo host never reaches this rung.
* Its `check` probe asks `rosidl_adapter.cli`, not `rosidl_adapter` — 1457's
  finding applied to the index describing the same rung. The package alone
  imports with none of its third-party deps present, so the old probe reported
  PRESENT for an interpreter that could not run `msg2idl.py`.

## What stays open

Issue 1483: moving the two entries out of `[python.*]` entirely, together with
the image derivation that must follow them. Nothing here forecloses it — the
`rosdep` keys are the data that move.
