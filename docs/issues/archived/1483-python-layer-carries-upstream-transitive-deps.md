---
id: 1483
title: "`[python.*]` enumerates two of upstream rosidl's own dependencies — and
  the tree already holds the non-drifting answer it was written to replace"
status: resolved
type: tech-debt
area: [tooling, orchestration]
severity: medium
found: 2026-09-24
related: [1482, 1481, 1457, 0368, phase-466]
---

**Home: [phase-466](../../roadmap/phase-466-scheduled-lanes-produce-verdicts.md)** — homed 2026-10-17; listed in that phase's "Issues homed here (2026-10-17)".

## The governing rule

A maintainer decision, arriving while 1482 was in flight:

> **Our index must NOT carry transitive dependencies. Where a package is a
> transitive dep of a direct apt ROS dependency, install the apt ROS package and
> let apt resolve it.**

The rule splits `[python.*]` by *whose* dependency an entry is, which is a
different question from "does apt have it" — the question 1481 answered.

## MEASURED — which entries are ours

Grepped over the tree, excluding `third-party/`, `target/` and the index itself.

| entry | direct consumer in THIS repo | verdict |
| --- | --- | --- |
| `catkin-pkg` | `packages/cli/colcon-cargo-ros2/colcon_nano_ros/workspace_bindgen.py:233,312` (`from catkin_pkg.package import parse_package`); `check-build-type-spelling.py` reasons about its `get_build_type()` | **ours** (also transitive for rosidl) |
| `pyyaml` | **11 of our own scripts** `import yaml` — including `check-workflow-indexed-apt.py`, `check-workflow-runner-isolation.py`, `check-required-contexts-reportable.py`, `check-interlock-visibility.py`, `check-ci-no-verb-fallback.py`, `gen-rosdep-snapshot.py`, `scripts/lib/workflow_commands.py`, `scripts/ci/lane-stage.py` | **ours** |
| `tomli` | `check-fixtures-manifest` and ~39 scripts' `import tomllib` / `import tomli` chain | **ours** |
| `colcon` | we ship colcon plugins | **ours** |
| `west`, `clang-format` | ours; no apt, already stated `apt_refused` exemptions | **ours** |
| `empy` | **no importer in this repo** — `grep '^\s*import em\b'` is empty | upstream rosidl's |
| `lark` | **no importer in this repo** — same grep | upstream rosidl's |

**Correction to the framing this issue was opened with.** The relay listed
`empy`, `lark` **and `pyyaml`** as "pure transitive deps of upstream's
`rosidl_adapter` / `rosidl_cli`", with "nothing in this repo imports them".
That is false for `pyyaml`: eleven of our own scripts import it, several of them
gates on the `check-fast` line, so a host that cannot `import yaml` cannot run
the lane this repo tells everyone to run before every push. `pyyaml` stays.

So the debt is **two entries, not five**: `empy` and `lark`.

## Why this is worth fixing even at two entries

Not the count — the *shape*. `[python.lark]` declares `apt = ["python3-lark"]`.
Upstream's rosdep key for the same thing is `python3-lark-parser`, and the
mapping from that key to `python3-lark` is data we already vendor. We are
hand-maintaining one edge of a dependency graph whose owner publishes it, and
the moment rosidl's humble branch changes a dep, our copy is silently wrong in
the direction that reads as correct.

## The hard case, stated plainly

The rule's remedy — "install the apt ROS package and let apt resolve it" —
**cannot apply here**, and that is the whole reason this layer exists. Issue
0368 / phase-327 created it for the **ROS-less host**, where
`msg_to_cyclone_idl.py` falls through to the vendored `third-party/ros/rosidl`
clone. There is no `ros-humble-rosidl-adapter` to install, because there is no
ROS apt repo. Something still has to name `empy` and `lark`.

## The answer, and the tree already has it

**Not a list of ours. `[source.rosidl]` should declare that its python deps come
from its OWN `package.xml`, resolved through `nros-rosdep-snapshot.toml`.**

Both halves are already here and both are already pinned:

* the **clone is pinned** (`humble-5621b26`), so upstream cannot change its
  declared deps under us without a pin move — and a pin move is a reviewed act
  with a forward-only rule already gated (`check-submodule-pins`);
* the **rosdep snapshot is pinned and vendored** (`nros-rosdep-snapshot.toml`,
  RFC-0099 D8 / phase-447 D3, 8103 lines, generated to a rosdistro commit and
  tracked, explicitly "cannot differ between machines, because it is a tracked
  file"). Every key rosidl_adapter's `package.xml` uses is in it, measured:

```
[key.python3-empy]        apt = ["python3-empy"]
[key.python3-lark-parser] apt = ["python3-lark"]        <- note the rename
[key.python3-catkin-pkg]  apt = ["python3-catkin-pkg"]
[key.python3-yaml]        apt = ["python3-yaml"]
```

So the answer to "what names them, if not us" is: **upstream names them, at a
commit we pinned, through a table we vendored at a commit we pinned.** Neither
list is ours and neither can move without a reviewed change. That is strictly
better than the `[python.*]` entries on the property the rule cares about, and
it is not a new mechanism — it is two existing pinned artifacts being joined.

`[source.rosidl]`'s own comment already names them in prose — "Python deps ride
`[python.*]`: catkin-pkg, empy (PINNED 3.3.4 — empy 4 breaks rosidl templates),
lark" — so the rung is already the thing that knows; it just delegates to a
hand-kept list instead of to upstream's file.

**I am not claiming a short list is unavoidable.** It is avoidable, with
machinery this repo already built for exactly this class of question.

## The one thing the derivation must keep

The `empy == 3.3.4` pin. It is OURS, not upstream's: empy 4.x breaks rosidl
templates, and a rosdep key carries no version. So the shape is "deps derived
from upstream's `package.xml` + rosdep snapshot, with our own version
constraints layered on top" — the constraint is a fact about our tolerance, the
dep list is a fact about upstream's code, and only the first belongs to us.
Anything that drops the pin while cleaning up the list makes codegen fail
silently on a host whose apt moved to 4.x.

## Ordering — do not remove the entries first

Issue 1482 made the runner image install the `[python.*]` entries that have no
`check.cmd`, **derived** from the index. That derivation is why this cleanup is
safe to do later: when `empy` and `lark` leave `[python.*]`, the image layer
follows automatically with no edit to `runner-container.sh`.

It is also why the order matters in one direction. The runner container is
`FROM ubuntu:22.04` with no ROS repo — it IS the ROS-less host this issue is
about — so **removing the two entries before `[source.rosidl]` supplies them
re-opens issue 1457 on that runner**, with the same `ModuleNotFoundError` and
no lane watching. Land the replacement first, or in the same change.

## What would close it

`[source.rosidl]` provisioning its own python deps from upstream's `package.xml`
through the rosdep snapshot, `empy` and `lark` gone from `[python.*]`, the
`empy` 3.3.4 constraint preserved, and a ROS-less host still able to run
`msg_to_cyclone_idl.py` — demonstrated in a container with no ROS, which is the
only host where any of this is load-bearing.

## Resolution (2026-10-23)

The maintainer chose the tracked-mirror shape over deriving at setup time.
Setup-time derivation fails on one point: `package.xml` lives in the rosidl
clone, and the runner-image generator and `nros setup --check` both run before
any clone exists.

- **`[python.empy]` and `[python.lark]` are gone.** They are now
  `[source.rosidl.python.python3-empy]` and
  `[source.rosidl.python.python3-lark-parser]`, each keyed by upstream's rosdep
  key with a `from` naming the declaring `package.xml`. Neither entry authors
  an apt name: `SdkIndex::python_layer` (Rust) and
  `index_packages.python_layer` (Python) take it from
  `nros-rosdep-snapshot.toml`. A key the snapshot does not map falls to pip,
  and the remedy says why.
- **The `empy` 3.3.4 pin stays, as ours.** It is `version` plus a required
  `version_why`. `SdkIndex::validate` refuses either one without the other, a
  key that is not `python3-*`, and one import declared both as ours and as a
  source's.
- **Consumers:** `nros setup --check` iterates the whole layer. So does
  `scripts/sdk/python-packages.py`, which generates the runner image's python
  layer: the image still derives the same five packages to the same apt names
  (`catkin-pkg empy lark pyyaml tomli`), so the image does not change.
- **`check-python-entry-provenance` now asks three things.**
  1. Every `[python.*]` entry is ours: it has an importer here, or it is a
     tool, and it carries no `rosdep` key.
  2. Every source key is in the snapshot.
  3. The mirror matches the clone's `package.xml` at the pinned ref in both
     directions: each mirrored key is an `<exec_depend>` of its `from`, and
     each `python3-*` `<exec_depend>` of those manifests is mirrored. With no
     provisioned clone it reports NOT VERIFIED through the skip ledger.

  The issue's text said rule 3 "needs the network". It does not: the clone
  `nros setup --source rosidl` writes is already at the pinned ref.
  Mutation-tested: pointing lark's `from` at `rosidl_adapter/package.xml`
  fails with "declares no `<exec_depend>python3-lark-parser</exec_depend>`".

**Demonstrated on a ROS-less host.** In a stock `ubuntu:22.04` container with
no `/opt/ros`, the clone mounted read-only, and the layer resolved and
installed by `python-packages.py --resolve`: all five resolved to apt, `catkin_pkg em lark yaml tomli` imported, and
`msg_to_cyclone_idl.py` turned `std_msgs/msg/Header.msg` into `Header.idl`.
`rosidl_cli`'s own keys (`python3-argcomplete`, `python3-importlib-metadata`)
were not installed, and the IDL path did not need them. That is why the mirror
reads only `rosidl_adapter` and `rosidl_parser`.
