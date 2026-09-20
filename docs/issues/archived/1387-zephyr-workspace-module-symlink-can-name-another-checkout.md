---
id: 1387
title: "a Zephyr west workspace inside one checkout can have its `nano-ros`
  module symlink pointing at ANOTHER checkout, so every image built there is
  half of each and nothing says so"
status: resolved
type: bug
area: [zephyr, tooling]
severity: high
found: 2026-09-18
related: [issue-1280, issue-0759, phase-455]
---

## What happened, measured

`/mnt/wd/data/projects/nano-ros-box` is a second checkout, used because ROS 2
lives in a distrobox and the host has a different libc and compiler. It carries
its own `zephyr-workspace/`. That workspace's west manifest module link was:

```
$ ls -l zephyr-workspace/nano-ros
zephyr-workspace/nano-ros -> /mnt/wd/data/projects/nano-ros      # the OTHER checkout
```

So `west build` inside the box checkout compiled:

* the **application** from this checkout —
  `APPLICATION_SOURCE_DIR:PATH=/mnt/wd/data/projects/nano-ros-box/examples/zephyr/rust/action-server`
* the **nano-ros Zephyr module** from the other one — the `Includes:` line of
  the configure names
  `/mnt/wd/data/projects/nano-ros/packages/platform/nros-platform-api/include`,
  `/mnt/wd/data/projects/nano-ros/packages/rmw/zenoh/zpico-sys/c/include` and
  `.../zpico-zephyr/include`, and `_NROS_RMW_DISPATCH_DIR`,
  `_NROS_MESSAGE_BOUNDS_DIR` and `_NROS_KNOB_INVENTORY_FILE` in `CMakeCache.txt`
  all resolve there.

One image, two trees, on two different branches. The Rust half was this
checkout's `nros-rmw-zenoh`; the C shim it calls was the other checkout's
`zpico.c`. Nothing warned. The symlink is dated 2026-08-15, so every Zephyr
image built in that workspace since has been mixed.

It was found only because a build log was read line by line for an unrelated
reason (phase-455 W4). The fix in the moment is one command:

```
ln -sfn "$PWD" zephyr-workspace/nano-ros      # then a pristine build
```

and a pristine build is required, not a reconfigure: the module root is a
configure-time identity, and the cached `_NROS_*_DIR` entries are what a
reconfigure would be asked to correct. After it,
`grep -c 'projects/nano-ros/packages' build.ninja` is 0 and
`projects/nano-ros-box/packages` is 760.

## Why this is not just operator error

It is a silent, durable, cross-checkout artifact mix, which is the class the
repo already refuses elsewhere: issue 0759 made the box refuse to work in the
host's tree, and issue 1280 made every path-valued variable re-root onto the
checkout being built when it names a DIFFERENT checkout. That rule — *outside
any checkout, keep; a different checkout, re-root; this one, keep* — is exactly
the rule this symlink breaks, and the west workspace is the one place it is not
applied, because the link is data in a gitignored directory rather than an
environment variable.

The dangerous property is the one the box memory already records: the failure
is not the loud `GLIBC_2.39 not found`. A mixed tree BUILDS, links and runs, and
the two halves only disagree when one of them changes.

## A second, smaller crossing in the same workspace

The same provisioning left the box checkout's own venv console scripts pointing
at the other checkout:

```
$ head -1 scripts/zephyr/.venv/bin/west
#!/mnt/wd/data/projects/nano-ros/scripts/zephyr/.venv/bin/python3
```

so `west` — and therefore `WEST_PYTHON` and `_Python3_EXECUTABLE` in every
Zephyr `CMakeCache.txt` — runs under the other checkout's interpreter. That one
is benign today (a build driver, not a compiler) and is left as measured rather
than fixed, but it is the same crossing and it survives the symlink repair.

## What would close it

1. A check that a resolved Zephyr workspace's `nano-ros` module link resolves
   to the checkout the build is being driven FROM, failing with the `ln -sfn`
   remedy. The marker walk `scripts/lib/checkout-paths.sh` already owns for
   issue 1280 answers "which checkout" — this is that predicate applied to one
   more path, not a new mechanism.
2. `just zephyr setup` writing the link from the resolved repo root rather than
   from `$PWD`-at-the-time, so the state cannot be created in the first place.
3. `just doctor` reporting it, since the existing workspaces on this disk are
   already in the bad state and nobody will re-run setup.

## What is NOT claimed

Which results this invalidated. Every Zephyr image built in that workspace
between 2026-08-15 and 2026-09-18 was mixed, and whether any of them was WRONG
depends on how far the two checkouts had diverged at each moment. The one
measurement taken here (phase-455 W4's goal-completion probe) was re-run from
scratch after the repair and reports the same numbers, which says nothing about
the other leaves.

## Resolution (2026-09-20)

### 1. The check — `just check zephyr-workspace-foreign-checkout`

`scripts/check-zephyr-workspace-foreign-checkout.py`, on the FAST line
(`just/check/platform.just`), so it is reached on `pull_request`,
`merge_group`, `push`, `schedule` and `workflow_dispatch`
(`check-default-gates-run-somewhere --survey`). Buildless, ~0.4 s: it reads one
config file, a glob of `CMakeCache.txt` and a directory of first lines, and
runs no west command.

It is issue 1280's rule applied to workspace DATA — outside any checkout KEEP,
this checkout KEEP, a DIFFERENT checkout FAIL — with "which checkout" answered
by the marker walk, never `.git` (issue 1336). The marker is READ out of
`scripts/lib/checkout-paths.sh` rather than restated, because 1280's own gate
refuses a fourth spelling of it.

**Three subjects, not one.** The link was only the live half:

1. the west manifest project (`<ws>/<[manifest] path>`) — what was measured here;
2. every `<ws>/build*/CMakeCache.txt` value — the DURABLE half. Repairing the
   link does not repair the images already built against it, which is why this
   issue's own remedy is a pristine build and not a reconfigure;
3. the venv console scripts' shebangs — the second crossing recorded above,
   which SURVIVES the link repair.

**Both the SPELLING and the resolved target are classified, and each subject
needs a different one.** The manifest project is a link inside this checkout
pointing at another, so only the target is foreign. The venv shebang is the
opposite: `#!/<other>/scripts/zephyr/.venv/bin/python3` resolves to
`/usr/bin/python3`, outside every checkout, so a realpath-only rule reports
nothing while the crossing is exactly the spelling that runs and the one that
lands in `WEST_PYTHON`. Measured: checking only the target missed all five of
the box venv's crossed scripts and 138 of its cached values.

**Negative controls on the normal path**, every invocation (issue 1280's gate's
rule — a control nobody runs decays into a comment). 14 of them, including the
two this issue names: plant a manifest symlink into a foreign checkout and the
gate fails; plant the legitimate self-pointing one and it passes; an unbound
manifest project (1258's shape) passes; an out-of-tree SDK path and this tree's
own path do not count; a bare checkout examines ZERO subjects.

**It skips rather than passing when there is nothing to look at.** With no
workspace and no venv it exits 78 and the recipe records a `nros_check_skip`
line. A gate that passes silently when its subject is absent is the failure
mode this repo keeps filing (issue 0650).

`check-zephyr-workspace-checkout.sh` — the tier-lane probe that front-runs the
phase-431 W1 ownership guard — now CALLS this tool with `--manifest-only`
instead of carrying its own copy of the walk. One implementation, two callers,
and the tier lane's scope is unchanged: it must not start failing on a stale
build dir it did not make.

`NROS_ALLOW_FOREIGN_BUILD_ARTIFACTS=1` silences subject 2 only, and say why —
the `NROS_ALLOW_SUBMODULE_REWIND` shape. Subject 2 is a statement about
ARTIFACTS, and the repair is a pristine rebuild of every affected leaf, which is
not a thing to demand of a contributor mid-review. Subjects 1 and 3 are
CONFIGURATION, cost an `ln` and a venv recreate, and stay fail-closed.

### 2. The creation side

Already closed, by issue 1258 / phase-449 W1: `scripts/zephyr/setup.sh:467-492`
no longer ends with `ln -sf "$NANO_ROS_ROOT"`. The manifest project is a plain
directory holding the manifest FILE and nothing else, so it carries no
`zephyr/module.yml` and is no `nros` module; each build names its own with
`-DZEPHYR_EXTRA_MODULES=<checkout>`. The state measured above is a PRE-1258
workspace.

One gap in that, fixed here: the repair only ever reached a workspace being
CREATED. The `Workspace exists, updating...` branch went straight to `west
update`, so a workspace carrying the old symlink stayed bound however often
setup was re-run — which is most of why the bad state survived a month. That
branch now calls `unbind-manifest-project.sh` too (idempotent; a project that is
already a plain directory is left alone).

### 3. `just doctor`

`_doctor-host` runs the same tool and prints one of three lines — `[OK] Zephyr
workspace names no other checkout`, `[INFO] no Zephyr workspace provisioned`, or
`[FAIL]` with the findings indented. Doctor is where someone asks whether their
machine is ready, and this issue's own point is that nobody re-runs setup.

### What the gate finds on this disk TODAY, read-only

The host checkout is clean — 76 subjects, no crossing. The box checkout is NOT,
and the repair recorded above was partial:

```
75 subject(s), 2 finding(s)
  - 353 cached value(s) across 69 of 69 build dir(s) name ANOTHER checkout
      NROS_REPO_DIR=/.../nano-ros          (the module root itself)
      WEST_PYTHON=/.../nano-ros/scripts/zephyr/.venv/bin/python3
      _Python3_EXECUTABLE=/.../nano-ros/scripts/zephyr/.venv/bin/python3
  - 5 venv console script(s) run ANOTHER checkout's interpreter
      .../nano-ros-box/scripts/zephyr/.venv/bin/{pip, pip3, pip3.14, pykwalify, west}
      #!/.../nano-ros/scripts/zephyr/.venv/bin/python3
```

So the `ln -sfn` fixed the link and nothing else: every image built in that
workspace between 2026-08-15 and the repair is still on disk, still mixed, and
the venv still crosses. **Anyone working in that checkout will see this gate go
red on its first `just check fast`, correctly**, with the two remedies above —
recreate the venv, and rebuild pristinely (or `NROS_ALLOW_FOREIGN_BUILD_ARTIFACTS=1`
while they decide).

### Still NOT claimed

Which results the month of mixed images invalidated. That is unchanged from what
this issue said when it was filed, and the gate does not answer it — it only
refuses to let the next one happen quietly.
