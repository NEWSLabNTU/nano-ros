---
id: 1457
title: "tier-2 nightly's zephyr module dies on `msg2idl.py failed on
  builtin_interfaces/msg/Duration.msg (exit 1)`, and the lane prints the exit code
  without the reason — a different stop from the four that preceded it"
status: open
type: bug
area: [ci, tooling, zephyr]
severity: high
found: 2026-09-22
related: [1389, 1360, 1158, 1387, 1482, 1481]
---

## What happens

Nightly run **35689793993** (schedule, 2026-09-22T05:12), job **106624135886**
(`tier 2 nightly (pairwise cover)`), step `just build tier2-nightly`. The lane
reaches its fixture build — `lane=tier2-nightly coords=35`, modules `esp32
freertos native nuttx qemu threadx_linux threadx_riscv64 zephyr` — and the
zephyr module fails:

```
== zephyr == FAILED (rc=2)
first error line(s) in …/tmp/build-test-fixtures-20260922-052701-405395/zephyr.log:
  123:  79:error: msg2idl.py failed on /tmp/tmpnus5wivz/builtin_interfaces/msg/Duration.msg (exit 1)
  124:  81:FATAL ERROR: command exited with status 1: /usr/bin/cmake --build \
        /home/runner/.nros/workspaces/zephyr/3.7/build-cpp-talker-cyclonedds
error: recipe `build-fixtures` failed with exit code 2
```

Four leaves report it (lines 123, 127, 207, 289 of the module log), all on the
same input: `builtin_interfaces/msg/Duration.msg`, a two-field message
(`int32 sec`, `uint32 nanosec`) that every other road in the tree converts
without complaint.

## Why this is a NEW stop, not one of the recorded ones

Tier 2's failures to date have all been EARLIER in the pipeline, and each has a
distinct signature this one does not match:

- **1389** — `entity-inventory schema version 6; this reader understands 3`.
- **1360** — `#error … codegen version the runtime does not accept`.
- **1158** — never reaching the cells, by provisioning or build STAGE.
- **1387** — `N cached value(s) across M build dir(s) name ANOTHER checkout`.

This run gets past all of them: `check-fast` is green, the fixture build starts,
seven modules are attempted and only `zephyr` stops. That is progress in the
lane and a new defect at the same time.

## What the log does NOT say, and why that matters

`msg2idl.py failed … (exit 1)` is a **status without a reason**. The wrapper
reports the child's exit code and drops its stderr, so the job log carries no
traceback, no missing import, no parse error — the same shape issue 1249 warns
about one layer down (`out="$(cmd)"` under `set -e`). Whatever msg2idl printed
died with the subprocess.

Two facts worth having before guessing:

1. the same run clones `rosidl` at `humble-5621b26` into the SDK store
   (`source humble-5621b26 — clone https://github.com/ros2/rosidl@5621b26…`), so
   `msg2idl.py` here is upstream's script at a pinned commit, not ours;
2. the input lives in a `/tmp/tmpnus5wivz/` staging tree the build materialises,
   so the failure may be about the staged file's surroundings (a missing
   `package.xml`, an empty parent) rather than the `.msg` content.

## What this is NOT

- **Not 1353.** No `No space left`, no truncation; the module log is written and
  quoted.
- **Not the C/C++ msg road in general.** `builtin_interfaces` is generated on
  every native leaf too, and those modules passed in this same run.
- **Not a pin move.** The `rosidl` clone is at the sha the SDK index pins.

## What would close it

The lane's zephyr module building `build-cpp-talker-cyclonedds` again. Before a
fix, one measurement: run `msg2idl.py` on that staged `Duration.msg` by hand and
capture its **stderr** — the wrapper's `exit 1` is not a diagnosis, and the next
person should not have to re-derive that. If the wrapper is ours, propagating
the child's stderr is worth doing whatever the root cause turns out to be.

## MEASURED — 2026-09-23, phase-466 W3

### The error

Nightly **35821404524** (2026-09-23T05:11), same job, same stop. The reason is
in the job log, in the module's `log tail` block rather than its quoted "first
error line(s)":

```
[2/551] msg_to_cyclone_idl unique_identifier_msgs/msg/UUID.msg
FAILED: [code=1] cyclonedds-ts/_idlroot/unique_identifier_msgs/msg/UUID.idl
Traceback (most recent call last):
  File "/home/runner/.nros/sources/rosidl/humble-5621b26/rosidl_adapter/scripts/msg2idl.py", line 17, in <module>
    from rosidl_adapter.cli import convert_files_to_idl
  File "/home/runner/.nros/sources/rosidl/humble-5621b26/rosidl_adapter/rosidl_adapter/cli.py", line 19, in <module>
    from catkin_pkg.package import package_exists_at
ModuleNotFoundError: No module named 'catkin_pkg'
```

### Two things this issue got wrong

**"The wrapper reports the child's exit code and drops its stderr."** It does
not. `run_adapter` in `scripts/cyclonedds/msg_to_cyclone_idl.py` writes
`result.stderr` before it exits, and the traceback above is that write. What
drops it is the FIXTURE RUNNER's report, which quotes lines matching `error:`
and nothing above them. Same reporting defect as issue 1458, and the reason
both issues were filed saying "the cause is not in the log" when it was.

**"the failure may be about the staged file's surroundings."** No. Nothing
about the `.msg`, the staging tree or `builtin_interfaces` is involved; the
adapter never gets as far as reading its argument. It is the same failure for
every message on that host.

### The cause

`/opt/ros/humble` does not exist on the self-hosted tier-2 runner — the job's
own log says `activate.sh: /opt/ros/humble/setup.bash not found` — so
`_adapter_bin_and_env` correctly falls through to the vendored rosidl clone
that `[rmw.cyclonedds]`'s `packages` provisions. That clone's python deps
(`catkin_pkg`, `empy==3.3.4`, `lark`, `PyYAML`) ride `[python.*]`, which is
REPORT-ONLY: `nros setup --check` names them and nothing installs them. They
are not installed on that runner.

So it is a HOST PROVISIONING GAP, not a code defect, and it does not belong to
the CI image work — that lane runs on `[self-hosted, linux, nros-qemu,
nros-sdk-zephyr, nros-big]`, not in `ci/docker/zephyr-ros`.

### What was fixed anyway, and what was not

`fix(#1457, phase-466)` makes the failure name its own remedy instead of
arriving as a traceback eleven minutes into a zephyr build:

* `_adapter_importable` probed `rosidl_adapter`, the PACKAGE; `msg2idl.py`
  imports `rosidl_adapter.cli`, and `cli` is where catkin_pkg and yaml are.
  The package imports with neither present, so the probe answered YES for an
  interpreter that could not run the script. It asks what the script asks now.
* The VENDORED rung returned as soon as the directory existed — the exact
  proxy the ROS rung's comment rejects three lines above it. It is probed now,
  and the refusal gained the third state it could not express: present-but-
  unusable, naming which deps are missing.

**This does not make the lane green.** Until the runner has those three
packages, tier-2 nightly will stop in the same place, with a refusal instead of
a traceback. Left OPEN for that reason; the remaining work is on the host, or
a decision that `nros setup --source rosidl` should provision its `[python.*]`
deps rather than report them.

### Negative control

Vendored clone present, interpreter a bare venv (the runner's shape).
Pre-fix tree:

```
adapter bin: /tmp/tmp.JtkRVPxGVo/rosidl/rosidl_adapter/scripts
```

accepted silently. Post-fix, same inputs:

```
REFUSED:
error: rosidl_adapter is not importable by this build's interpreter.
  ...
  The vendored copy is present (/tmp/tmp.Bl982sn2Mb/rosidl) but
  `import rosidl_adapter.cli` fails under it — its python deps
  are missing: catkin_pkg, empy==3.3.4, lark, PyYAML.
```

## CORRECTED — 2026-09-24, issue 1482

Everything above about the CAUSE holds. The sentence about the REMEDY does not:

> They are not installed on that runner. So it is a HOST PROVISIONING GAP […]
> the remaining work is on the host

That framing is overridden. **A self-hosted runner here is a container**
(`scripts/ci/runner-container.sh`), and when it lacks something the fix is the
IMAGE, never the host environment. The host answer was never available anyway:
the running container is `--cap-drop ALL --security-opt no-new-privileges` with
a non-root user, so no job can install a system package, and
`runner-provision.sh` never sudoes and never installs one either — which leaves
the generated Dockerfile as the only producer there has ever been.

Nor was it three pip installs. It is four modules (`catkin_pkg`, `em`, `lark`,
`yaml`), and issue 1482 measured that `ubuntu:22.04` packages **all four in
apt**, universe, no ROS repo — including `python3-catkin-pkg 0.4.24-2`, which
this issue's own reading had assumed was ROS-repo only.

The second option floated above — "a decision that `nros setup --source rosidl`
should provision its `[python.*]` deps rather than report them" — is also
declined, for the reason the layer is report-only in the first place: a
provisioning step that pip-installs into whatever interpreter it finds is issue
1481's shadowing hazard, and on a runner it would put the modules in a volume
where nothing says they are there. The image states them.

**Fixed in 1482**: `runner-container.sh` now resolves the `[python.*]` layer
from `nros-sdk-index.toml` through `scripts/sdk/python-packages.py`, the same way
it already resolved `[prereq.*]`, with the apt/pip split measured inside the
image rather than on the workstation that generated it.

**Left OPEN** for one reason only, and it is not a code change: the runner's
image has to be rebuilt and the container restarted before the lane can be green.

```sh
scripts/ci/runner-container.sh nros-qemu,nros-sdk-zephyr,nros-big --build
scripts/ci/runner-container.sh nros-qemu,nros-sdk-zephyr,nros-big --run
scripts/ci/runner-doctor.sh   nros-qemu,nros-sdk-zephyr,nros-big
```

Close this when a tier-2 nightly gets past `msg_to_cyclone_idl` on that runner.

## The SYMPTOM CHANGED — the probe now refuses up front (2026-09-25 nightly)

Nightly **36097564895** (05:12 schedule), job **107952942994**, `tier 2 nightly
(pairwise cover)`, step `just build tier2-nightly`. Same lane, same job, the
same four cyclonedds leaves — and a different line:

```
  145:  69:error: rosidl_adapter is not importable by this build's interpreter.
  146:  81:FATAL ERROR: command exited with status 1: /usr/bin/cmake --build \
        /home/runner/.nros/workspaces/zephyr/3.7/build-cpp-service-server-cyclonedds
  147:  69:error: rosidl_adapter is not importable by this build's interpreter.
  148:  81:FATAL ERROR: ... build-cpp-talker-cyclonedds
  149:  69:error: rosidl_adapter is not importable by this build's interpreter.
  150:  81:FATAL ERROR: ... build-cpp-action-client-cyclonedds
```

**This is worth recording because the text a reader would grep for is gone.**
The section above documents `msg2idl.py failed on …/builtin_interfaces/msg/
Duration.msg (exit 1)` — msg2idl dying several frames deep, with the real cause
(`rosidl_adapter.cli` importing catkin_pkg and yaml) only visible in the
traceback. Tonight the build refuses at the PROBE instead, before invoking
msg2idl at all, and says so in one line. Anyone searching this issue's original
error string against a current log will find nothing.

That is the `_adapter_importable` probe this issue's own analysis names, now
reaching the condition it was written for. The diagnostic improved; **the
underlying dependency gap did not close** — the same three leaves fail, in the
same job, in the same lane.

**What this does NOT say.** It does not say the probe is wrong — refusing early
with a clear message is better than a traceback, and if anything it strengthens
the case that the remedy belongs where the interpreter's packages are chosen.
It also does not re-open the question of WHICH package is missing: this run's
message names importability, not a package, so the catkin_pkg/yaml analysis
above is neither confirmed nor refuted by it.
