---
id: 1481
title: "`nros setup --check` told every host to `pip3 install --user` a package
  six of eight entries get from apt — and a `~/.local` copy shadows the build the
  `ros-<edition>-*` packages were compiled against"
status: resolved
type: bug
area: [tooling, orchestration, ci]
severity: medium
found: 2026-09-24
related: [0368, 1457, 1128]
---

## The governing rule

A maintainer decision, recorded here because it constrains more than one file:

> **The `ros-<edition>-*` apt packages are built against the dependency versions
> Ubuntu locks. Do not install pip copies of those deps.** `pip3 install --user
> <x>` puts a copy in `~/.local/lib/pythonX/site-packages`, which PRECEDES
> `/usr/lib/python3/dist-packages` on `sys.path`, so it *shadows* the very build
> ROS was compiled against. pip is legitimate only where apt provides nothing.

## What happened

`nros-sdk-index.toml`'s `[python.*]` layer had eight entries, all pip-only, and
`cmd/setup.rs` reported every one of them with an unconditional remedy:

```rust
format!("pip3 install --user {}{}", py.pip, …version…)
```

Six of the eight are apt-provided. Measured with `apt-cache policy` on the
maintainer's jammy/arm64 host (with the ROS 2 apt repo configured):

| entry | pip spelling printed | apt package | version | origin |
| --- | --- | --- | --- | --- |
| catkin-pkg | `catkin_pkg` | `python3-catkin-pkg` | 1.1.0-101 | packages.ros.org/ros2 |
| colcon | `colcon-common-extensions` | `python3-colcon-common-extensions` | 0.3.0-100 | packages.ros.org/ros2 |
| pyyaml | `PyYAML` | `python3-yaml` | 5.4.1-1ubuntu1 | ubuntu main |
| empy | `empy==3.3.4` | `python3-empy` | **3.3.4-2** | ubuntu universe |
| lark | `lark` | `python3-lark` | 1.1.1-1 | ubuntu universe |
| tomli | `tomli` | `python3-tomli` | 1.2.2-2 | ubuntu universe |
| west | `west` | — none — | | pip is correct |
| clang-format | `clang-format==17.0.6` | (apt has 1:14.0-55~exp2) | | pip is correct |

**`empy` is the sharpest case.** The index hand-pins `version = "3.3.4"` with
the comment "empy 4.x is INCOMPATIBLE, keep the pin" — and Ubuntu's
`python3-empy` **is already exactly 3.3.4-2**. The printed remedy installed a
shadowing duplicate of the correct version, for no gain, on the one package
where a wrong major version breaks rosidl templates.

**The remedy has been followed, and the shadow is live.** On this host, apt has
`python3-catkin-pkg`, `python3-yaml` and `python3-colcon-common-extensions`
installed, and `import` resolves all three from `~/.local`:

```
catkin_pkg: /home/aeon/.local/lib/python3.10/site-packages/catkin_pkg/__init__.py
yaml:       /home/aeon/.local/lib/python3.10/site-packages/yaml/__init__.py
colcon:     /home/aeon/.local/lib/python3.10/site-packages/colcon/__init__.py
em:         /usr/lib/python3/dist-packages/em.py          ← apt's, correctly
lark:       /usr/lib/python3/dist-packages/lark/__init__.py
```

The versions coincide today, which is why nobody noticed.

**The tree already contradicted itself.** `ci/docker/ci-base/Dockerfile`
apt-installs `python3-tomli` while the index told a human to
`pip3 install --user tomli`. So does the book: its Debian/Ubuntu block
(`book/src/getting-started/installation.md`) installs
`python3-catkin-pkg python3-empy python3-lark python3-yaml`, and its pip lines
are the **Fedora, Arch and macOS** blocks, each saying in a comment that those
distros ship no rosidl packages. The documented flow was right; the tool
disagreed with it.

**And five of the eight printed no remedy at all.** `catkin-pkg`, `empy`,
`lark`, `pyyaml` and `tomli` declared no `check`, so `--check` reported
`[UNPROBED]` and the remedy string was never reached — which is a layer that
answers nothing, and part of why a wrong remedy survived this long.

## The fix

### 1. A `[python.*]` entry declares apt beside pip

Shape reused from the `[prereq.*]` rows rather than invented
(`ManagerPackages`, RFC-0099 D9 / phase-447 D2): a flat list, or
`{ default = [..], <release> = [..] }`.

**No entry needs the per-release split today, and none is written.** The
resolver asks the HOST's apt for a candidate before printing an apt remedy, so a
release that does not package one falls back by MEASUREMENT rather than by a
table someone has to keep current. The table shape stays available for what
measurement cannot reach (a rename we must anticipate), and an explicit
`<release> = []` still reads as "not packaged there".

`dnf`/`pacman`/`brew` are deliberately **not** mapped. The book already measured
that Fedora and Arch ship no rosidl packages, and nothing here has measured the
rest. A guessed mapping prints a command that fails; an absent one falls to pip,
which is what those hosts already do.

### 2. Every entry STATES its apt position

`apt = [..]` **xor** `apt_refused = "<why>"`, never silence — the same
per-field rule as the sizing descriptor's `[<section>.refused]`: a value, or a
reason, never a silent default. Gated in `SdkIndex::validate`, whose subject is
DERIVED (it iterates `self.python`; there is no authored list and no exemption),
with two mutation controls in `sdk_index.rs`'s tests — an entry that states
nothing, and an entry that states both.

That is what keeps `west` and `clang-format` from *looking* overlooked:

* `west` — "PyPI only; measured, `apt-cache policy python3-west` is empty on
  jammy, and Zephyr's own docs install it from PyPI".
* `clang-format` — "the pin is exact and apt cannot meet it (jammy ships
  1:14.0-55~exp2), and `just setup-clang-format` wants the wheel's STANDALONE
  binary under `build/clang-format/bin`, not a host-wide install".

### 3. The remedy prefers apt, by measurement, and says why when it does not

`orchestration::python_provider` is the ONE spelling of the rule:

1. entry declares no apt names for this release → pip, naming the refusal or
   "not packaged on this release";
2. no `apt-cache` on this host → pip ("this host has no apt") — Fedora, Arch,
   macOS;
3. `apt-cache policy <pkg>` has no candidate → pip, naming the package. **This
   is the ROS-less host**, and it is why `catkin-pkg` and `colcon` need no
   special case: their apt names exist only on packages.ros.org/ros2, so a host
   without that repo measures "no candidate" and gets pip — exactly the case
   issue 0368 / phase-327 created this layer for. `empy`/`lark`/`tomli`/`pyyaml`
   come from plain Ubuntu, so they resolve to apt on any Ubuntu host, with or
   without ROS;
4. a `version` pin the candidate does not satisfy → pip, naming both. Matching
   is conservative: `3.3.4-2` and `3.3.4+ds1-1` satisfy `3.3.4`, `3.3.4.1` and
   `3.3.40` do not, and anything unreadable loses — a duplicate is cheap, a
   wrong-major `empy` is broken codegen;
5. otherwise → `sudo apt-get install -y <names>`, composed by the existing
   `native_install_command`.

An unknown package makes `apt-cache policy` print **nothing** with a **zero**
exit status (measured), so the exit status cannot be the discriminator; the
parser keys on the output.

### 4. A shadow is REPORTED, and fails the check

When the entry resolves to apt, every declared apt package is installed, and the
interpreter resolves the module from its user-site directory, `--check` prints

```
  [SHADOWED] python catkin-pkg (catkin_pkg) — apt's python3-catkin-pkg is installed,
             but `import catkin_pkg` resolves /home/…/.local/lib/python3.10/site-packages/catkin_pkg/__init__.py
             (run: python3 -m pip uninstall catkin_pkg — apt's copy then answers)
```

and bails with its own sentence. Deliberately **not** folded into `missing` or
`broken`: nothing is absent and nothing is broken, and the summary line would
then say something false. The defect is *which copy answers*.

Origin comes from the INTERPRETER (`importlib.util.find_spec`), not from the
filesystem — RFC-0097 D12's rule for `python_import`, one module over.

### 5. A probe-less entry is probed by importing its module

`module` is authored only where it differs from `pip` (`PyYAML` → `yaml`,
`empy` → `em`, `colcon-common-extensions` → `colcon`) and DERIVED otherwise. An
entry with no `check` is now probed by importing it, so the five `[UNPROBED]`
entries answer. Consequence, stated because it is a behaviour change:
`nros setup --check` now exits non-zero on a host genuinely missing one of them.
No merge-gating lane runs `nros setup --check`; `just doctor` greps for
`[BROKEN]`, which `[SHADOWED]` deliberately is not.

## The sweep

`git grep -nE 'pip3? install'` outside `third-party/`, minus archived issues and
roadmap history:

| site | verdict |
| --- | --- |
| `packages/cli/nros-cli-core/src/cmd/setup.rs` | **fixed** — the defect |
| `scripts/cyclonedds/msg_to_cyclone_idl.py` (2 remedies) | **fixed** — both spelled `pip3 install --user catkin_pkg 'empy==3.3.4' lark`, all three apt-provided. They now point at `nros setup --check`, which applies the rule, rather than carrying a second copy of it |
| `book/.../installation.md` Debian/Ubuntu block | already apt — the model |
| `book/.../installation.md` Fedora / Arch / macOS blocks | correct as written; those distros ship no rosidl packages and say so |
| `ci/docker/ci-base/Dockerfile` | clean — no pip at all, and apt-installs `python3-tomli` |
| `ci/docker/zephyr-ros/Dockerfile:231` | **left alone, deliberately — see below** |
| `west`, `clang-format`, `kconfiglib`, `uv`/venv provisioning, `pyelftools`, `pykwalify`, `jsonschema`, `packaging` | no apt package, or a deliberate project-local venv |
| `just/px4.just:27` | documents having REMOVED a pip install |

### The one recorded decision: `ci/docker/zephyr-ros/Dockerfile`

That image is `FROM ros:humble-ros-base`, where apt's `python3-yaml` is 5.4.1 —
the version ROS humble was built against — and the Dockerfile pip-installs
`PyYAML==6.0.2` over it. That is the maintainer's rule being broken inside our
own image, and it is **not** changed here, for reasons that are measurable and
were not measurable from this checkout:

* Zephyr's `requirements-base.txt` is the constraint, and Zephyr is not vendored
  in this tree, so what PyYAML version Zephyr needs **could not be measured
  here**. Guessing in the direction of "5.4.1 is surely enough" risks the Zephyr
  build to satisfy a rule about the ROS half.
* The block is gated against `scripts/check-python-deps.py` by
  `scripts/check-ci-image-python-deps.py`, whose rule is "every pip name in the
  `west` + `zephyr-build` groups appears in the Dockerfile's pip block, PINNED".
  Satisfying the apt rule means teaching that gate that a group member can be
  satisfied by the BASE IMAGE's apt — a claim the gate cannot verify, since our
  Dockerfile has no apt line for it. That needs its own change and its own
  measurement.
* The other four in the block (`west`, `pyelftools`, `pykwalify`,
  `jsonschema`) have no apt package; pip is right for them.

So: recorded, not silently fixed, and not silently left. Whoever picks it up
needs a Zephyr `requirements-base.txt` reading first.

### Why the apt names here are NOT under `check-workflow-indexed-apt`

That gate's remedy is `python3 scripts/sdk/prereq-packages.py --manager apt
<key>`, which resolves `[prereq.*]` keys. A `[python.*]` key is not one, so
extending its subject would produce a remedy nobody can run — worse than no
gate. The gate that holds this line is the validator (§2), whose subject is the
`[python.*]` table itself.

## Verification

Negative control, on the pre-fix tree (`nros setup --check | grep python`):

```
  [UNPROBED] python catkin-pkg (catkin_pkg)
  [OK]      python  clang-format (clang-format)
  [OK]      python  colcon (colcon-common-extensions)
  [UNPROBED] python empy (empy)
  [UNPROBED] python lark (lark)
  [UNPROBED] python pyyaml (PyYAML)
  [UNPROBED] python tomli (tomli)
  [OK]      python  west (west)
```

Five entries answering nothing, three shadows unreported. After:

```
  [OK]      python  catkin-pkg (catkin_pkg)
  [SHADOWED] python catkin-pkg (catkin_pkg) — apt's python3-catkin-pkg is installed, but `import catkin_pkg` resolves /home/aeon/.local/lib/python3.10/site-packages/catkin_pkg/__init__.py
            (run: python3 -m pip uninstall catkin_pkg — apt's copy then answers)
  [OK]      python  clang-format (clang-format)
  [OK]      python  colcon (colcon-common-extensions)
  [SHADOWED] python colcon (colcon-common-extensions) — apt's python3-colcon-common-extensions is installed, but `import colcon` resolves /home/aeon/.local/lib/python3.10/site-packages/colcon/__init__.py
            (run: python3 -m pip uninstall colcon-common-extensions — apt's copy then answers)
  [OK]      python  empy (empy)
  [OK]      python  lark (lark)
  [OK]      python  pyyaml (PyYAML)
  [SHADOWED] python pyyaml (PyYAML) — apt's python3-yaml is installed, but `import yaml` resolves /home/aeon/.local/lib/python3.10/site-packages/yaml/__init__.py
            (run: python3 -m pip uninstall PyYAML — apt's copy then answers)
  [OK]      python  tomli (tomli)
  [OK]      python  west (west)
```

The three named are exactly the three measured by hand at the top of this issue.
`empy` and `lark` resolve from `/usr/lib/python3/dist-packages` and are
correctly silent.

Nothing was installed to verify this: the change is exercised by asserting on
the remedy STRINGS and the resolution logic (`orchestration::python_provider`
has a fake `AptQuery`), because `pip3 install --user` on this host would create
the exact shadowing the change exists to prevent. `apt-cache policy` is
read-only.

## Files

* `nros-sdk-index.toml` — `[python.*]`: `apt` / `apt_refused` / `module` on all
  eight, with the rule and the measurements recorded above the table.
* `packages/cli/nros-cli-core/src/orchestration/python_provider.rs` — new; the
  resolver, the version-pin rule, the `apt-cache policy` reader, the
  interpreter-side shadow probe.
* `packages/cli/nros-cli-core/src/orchestration/sdk_index.rs` — `PythonDep`
  fields + `module()` + `apt_packages()`; the xor rule in `validate` and its two
  mutation controls.
* `packages/cli/nros-cli-core/src/cmd/setup.rs` — `python_remedy`, the probe
  fallback, the `[SHADOWED]` report and its bail.
* `scripts/cyclonedds/msg_to_cyclone_idl.py` — two hard-coded pip remedies now
  point at the rule instead of restating it.
