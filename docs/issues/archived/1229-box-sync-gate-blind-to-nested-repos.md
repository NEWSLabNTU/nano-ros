---
id: 1229
title: "`check-box-sync-covers-tracked-source` swept ONE index while rsync copies
  the whole tree, so the fifth `target/` incident walked past it — and a sixth
  was already live in another repo"
status: resolved
type: bug
area: [ci, build, process]
related: [0400, 0401, 0759, 1046, 1055]
---

## What

`scripts/dev/ros2-box-sync.sh` mirrors this checkout into the ROS distrobox's
own tree and excludes build output by NAME pattern. One of those patterns is
`--exclude 'target/'`, a **Rust** build-output name. Zephyr has two directories
of **source** with that name:

```
zephyr-workspace/zephyr/drivers/i2c/target/
zephyr-workspace/zephyr/include/zephyr/drivers/i2c/target/
```

Five files, I²C target (slave) mode. Without them the mirror could not
**configure any Zephyr image at all**:

```
drivers/i2c/Kconfig:101: 'drivers/i2c/target/Kconfig' not found
```

— which reads as a broken Zephyr checkout, not as a sync rule. That is the
**fifth** time a build-output pattern in this file has eaten tracked source; the
first four are enumerated in the script's own header and in issue 1055.

## The defect this issue is actually about

A gate exists for exactly this class: `check-box-sync-covers-tracked-source`,
written for the fourth incident. It did not fire on the fifth, and it was green
the whole time.

It swept `git ls-files` — the **superproject's index**. `zephyr-workspace/zephyr`
is a nested repository (west-managed, and `zephyr-workspace` is gitignored here),
so nothing it tracks appears in that index. Measured on `main` before the fix:

```
$ git ls-files | grep -cE '(^|/)target/'
0
$ find zephyr-workspace -type d -name target -not -path '*/build-*' | wc -l
2
```

Zero against two. rsync copies the working **tree**, so what reaches the mirror
is not a property of one index. A sweep scoped to one repository is blind to
every sibling's source — and the siblings are where most of the bytes are.

The nested-repo explanation was **confirmed, not assumed**: with a nested
repository restored and the re-include removed, the widened sweep reports the
loss by name; the superproject-only sweep reports `OK`.

## The second instance, found the moment the sweep could see

Widening it turned up a live case in a completely unrelated tree:

```
third-party/px4/PX4-Autopilot/boards/modalai/voxl2/target/
    voxl-px4  voxl-px4-start  voxl-px4-hitl  voxl-px4-hitl-start
    voxl-px4-fake-imu-calibration.config
    voxl-px4-hitl-set-default-parameters.config
```

Upstream board source — PX4's own `boards/modalai/voxl2/scripts/install-voxl.sh`
`adb push`es each of those six files **by that path**, and PX4's `.gitignore`
says nothing about `target`. So `zephyr-workspace/zephyr` was not special; it
was simply the tree whose loss produced a loud enough failure to investigate.

## What was measured, over which trees

A provisioned checkout carries **39 repositories** the mirror copies: the
superproject, 20 declared submodules (plus their own nested submodules), and 12
west projects under `zephyr-workspace`. Scanning every one of them for a
directory matching any build-output pattern in the sync script:

| tree | matching directories | verdict |
| --- | --- | --- |
| `zephyr-workspace/zephyr` | 6 | 4 `build/` + 2 `target/`, all **source**, all now re-included |
| the other 11 west projects | 0 | — |
| `third-party/px4/PX4-Autopilot` | 1 | `boards/modalai/voxl2/target/`, **source**, now re-included |
| every other submodule | 0 | — |
| `packages/cli/third-party/play_launch` | 2 | Rust `target/` build output, correctly excluded |

So the exclusion list drops tracked source in exactly two of the nested trees,
and both are fixed here.

## The fix

`scripts/check-box-sync-covers-tracked-source.py` now sweeps **every repository
the mirror copies**, not one.

**The contract, stated:** a mirror is faithful when no repository's tracked
source is dropped. Not "every file matching `target/`" — the next collision will
be a different pattern, and the script's header already records `build`,
`build-*`, `build-*/` and `/examples/workspaces/*/Cargo.toml`. And not "every
file a build needs", because which files a given build needs is not knowable
from here; faithful-copy is the rule that can be enforced, and it is the rule
all five incidents violated.

Discovery is a directory walk that descends **only where rsync would**: a
directory whose first matching rule is an `--exclude` is not mirrored, so
nothing inside it can be lost and nothing inside it is visited. That prune is
what makes the walk ~1.2 s on a fully provisioned tree instead of minutes — the
~40 GB of build output is never touched — and it needs no second list, because
the rules already name it.

Matching is memoised per **directory**: a directory-only rule can never match at
a file (it needs a `/` after the run), so a file's fate is decided by its
ancestors, which are evaluated once each rather than once per file. The three
rules that can match at a file are all anchored at the root, so they cost a
`str.startswith` per path. The self-test cross-checks the memoised path against
the rule-by-rule scan on ten real paths — a fast path that disagrees with the
slow one is a gate reporting about a rule set nobody wrote.

## Three outcomes, because "not here" is not "fine"

Agent worktrees and every CI lane carry neither `zephyr-workspace` nor an
initialised submodule, so a disk-scoped sweep would silently compare nothing —
the shape issue 1043 fixed one gate over. So:

* **FAIL** — a tracked path was measured to be dropped.
* **NOT VERIFIED** — a tree is absent here. Named, never silent, and recorded in
  the shared `nros_check_skip` ledger so `just check`'s closing sentence says
  what it did not sweep. Derived, not listed: an anchored `--include` names the
  tree its carve-out protects, and `.gitmodules` names the rest.
* **OK** — measured, nothing dropped.

`NROS_BOX_SYNC_SWEEP_STRICT=1` escalates a skip to a failure, for a lane that
really does provision every tree. Setting it on a lane that provisions a subset
re-creates 1043.

## Proof it catches the original defect

The mutation runs on the **normal path**, every invocation (phase-395): a
throwaway superproject with a nested repository holding
`drivers/i2c/target/Kconfig`, swept twice — once against a rule set with the
re-include and once without. Without it the sweep must report the nested repo's
file by name; with it, nothing. A discovery that finds fewer than two trees is
also a failure, since that is the blind spot itself.

End to end, against a real nested repository and the real sync script with the
re-include deleted:

```
check-box-sync-covers-tracked-source: 3 TRACKED path(s) would not reach the box mirror
  zephyr-workspace/zephyr/drivers/i2c/target/CMakeLists.txt
      excluded by  --exclude 'target/'  (tracked by zephyr-workspace/zephyr)
  zephyr-workspace/zephyr/drivers/i2c/target/Kconfig
      excluded by  --exclude 'target/'  (tracked by zephyr-workspace/zephyr)
  zephyr-workspace/zephyr/include/zephyr/drivers/i2c/target/eeprom.h
      excluded by  --exclude 'target/'  (tracked by zephyr-workspace/zephyr)
```

Restoring the re-include returns `OK (… across 3 source tree(s))`.

## Lane and cost

Unchanged: `check-fast`, source-only, no rsync, no box, no network. Measured
0.63 s in a bare agent worktree (one repository, both mutations included) and
~1.2 s of walk plus one `git ls-files` per repository on a provisioned host —
paid only where the trees exist, which is the only host the box sync runs on.

## Not done here

`check-hook-repo-side-effects` enumerates `scripts/*.sh` only, so a **Python**
gate that builds a throwaway repository is outside its reach. This script clears
`GIT_DIR` & co. itself (issue 0986's hazard) and says so, but the gate's reach
is narrower than the rule it enforces — the 0196 shape, one series over.
