---
id: 1379
title: "The store's Zephyr workspace holds ONE `build-<leaf>` dir per example,
  shared by every checkout, so a west build in one worktree compiles another
  clone's absolute source paths"
status: resolved
type: bug
area: [build, zephyr, testing]
severity: high
found: 2026-09-17
related: [issue-1248, issue-1280, issue-0925, issue-1243, issue-1258, issue-1253, phase-449]
resolved_in: "every configuring `west build` names its own checkout's module, and the module/application disagreement is refused at configure time"
---

## What happens

`just zephyr build-one rust/listener zenoh` in a fresh agent worktree built
into `/home/aeon/.nros/workspaces/zephyr/3.7/build-rust-listener-zenoh`, and
the C half of that build compiled sources from a DIFFERENT checkout:

```
[1271/1315] Building C object modules/nros/CMakeFiles/nros.dir/
  home/aeon/repos/simple-autoware-safety-island/third-party/nano-ros/
  packages/rmw/zenoh/zpico-sys/c/zpico/zpico.c.obj
```

The Rust half correctly used the invoking checkout
(`/home/aeon/nros-agent-worktrees/392-b/...`), so the image being linked was
half one tree and half another. Nothing warned.

## Why it happens

`scripts/lib/zephyr-workspace.sh`'s resolution ladder ends at
`$NROS_STORE/workspaces/zephyr/<version>` — the STORE, which is shared by every
checkout on the host — and the build directory inside it is named from the LEAF
only (`build-<example>-<rmw>`). Two checkouts building the same example
therefore target the same cmake build dir. cmake caches absolute source paths
at configure time and does not reconfigure merely because a different tree
invoked it, so the second checkout inherits the first one's paths for every
target whose cache entry did not change.

This is the failure mode CLAUDE.md's "BUILD WHERE YOU RUN" rule exists to
prevent, arrived at from the other direction: `ros2-box-sync.sh` was retired
because provisioned source and build output share directories, and this is the
same sharing one layer up. The store arm is deliberately LAST in the ladder
(phase-440 W1 / RFC-0095 D4) so that nothing MOVED when it landed — but for a
checkout with no in-tree workspace, last is the only arm that resolves.

## Why it is high severity

It is silent and it is cross-tree. A worktree's build can be linked against
another clone's sources — including sources from a branch that clone is
mid-edit on — and the resulting image passes or fails for reasons belonging to
a tree nobody was looking at. With several agent worktrees on one box (which is
how this repo is worked) two sessions can also fight over the same build dir.

## What was not measured

Whether the Rust/C split above is the general shape or just what this build
happened to reach first, and whether a `cmake <build-dir>` reconfigure repairs
it (`just reconfigure-stale` is the tool, and this is exactly a
generated-build-file problem, so it should be tried before anyone reaches for
`rm -rf` — CLAUDE.md's rule applies).

## Direction

The build dir has to carry the checkout's identity, not just the leaf's — e.g.
`build-<leaf>-<hash of the resolved repo root>` — or the store workspace has to
stop being a build location and only be a SOURCE location, with builds landing
under the invoking checkout. The second is closer to what "build where you run"
means; the first is the smaller change.

Found while making the phase-392 amendment B measurement
([`docs/roadmap/phase-392-static-memory-space-campaign.md`](../roadmap/phase-392-static-memory-space-campaign.md)),
which had to abandon a Zephyr measurement image because of it.


## Resolution (2026-09-20)

### The mechanism is NOT the one this issue guessed

"Why it happens" above says cmake caches absolute source paths at configure
time and a second checkout inherits the first one's. That does not survive
measurement. The build directory the report came from
(`~/.nros/workspaces/zephyr/3.7/build-rust-listener-zenoh`) was configured
entirely by the invoking checkout:

```
APPLICATION_SOURCE_DIR:PATH=/home/aeon/nros-agent-worktrees/392-b/examples/zephyr/rust/listener
CMAKE_HOME_DIRECTORY:INTERNAL=/home/aeon/nros-agent-worktrees/392-b/examples/zephyr/rust/listener
_NANO_ROS_CODEGEN_TOOL=/home/aeon/nros-agent-worktrees/392-b/packages/cli/target/release/nros
NROS_SHARED_CARGO_ROOT=/home/aeon/nros-agent-worktrees/392-b/build/corrosion-cargo/zephyr
NROS_REPO_DIR:PATH=/home/aeon/repos/simple-autoware-safety-island/third-party/nano-ros
```

Every entry the invoking build *set* names the invoking checkout. The single
entry that is *derived* — `NROS_REPO_DIR`, which `zephyr/CMakeLists.txt`
computes as the parent of the `nros` MODULE — names another clone. The module
row confirms it:

```
"nros":"/home/aeon/.nros/workspaces/zephyr/3.7/nano-ros":"…/simple-autoware-safety-island/third-party/nano-ros/zephyr"
```

and `<store>/workspaces/zephyr/{3.7,4.4}/nano-ros` are both symlinks to that
clone, dated the day the store was provisioned. So this is **issue 1258's
mechanism**, not a stale build directory: the workspace's west manifest project
binds every build in that workspace to the checkout that ran `just zephyr
setup`, and `NROS_REPO_DIR` carries that binding into every `zephyr/`,
`packages/platform/**`, `packages/api/**` and `cmake/**` path the module
compiles. It would have repeated in a freshly created build directory. Keying
the build directory by checkout — this issue's own smaller suggestion — would
not have fixed it.

The build directory being shared IS real, and it is handled below; it is just
not what produced the reported image.

### What was fixed

**1. Every configuring `west build` names its own checkout's module.**
phase-449 W1 landed the right fix for three builders (`zephyr-fixture-run-one.sh`,
`check-copy-out.sh`, `tests/zephyr/run-c.sh`) and stopped there. There were
seven, and `just zephyr build-one` — the exact command this issue reports — was
one of the four left behind, along with both FVP recipes in
`just/zephyr-setup.just`. That is CLAUDE.md's "fix the CLASS, not the reported
site" paid for again, and the sweep is now one helper,
`scripts/lib/zephyr-module.sh`, used by all seven.

**2. `NROS_REPO_DIR` is FORCEd.** A `set(... CACHE ...)` without `FORCE` does
not overwrite an entry the cache already holds — measured on a two-configure
toy whose literal changed between runs and which reported the first value both
times. So a build directory ever configured against a foreign module would keep
compiling that tree's sources through `${NROS_REPO_DIR}` even after the module
binding was corrected: the same contamination, surviving its own fix. The value
is derived, never a knob (nothing in the tree passes `-DNROS_REPO_DIR`;
consumers read the ENV variable of the same name, which this does not touch).

**3. The mix is refused at configure time**, in
`zephyr/cmake/nros_checkout_guard.cmake`, called from the module itself. The
builders and the gate cover the routes this repository owns; a `west build`
typed by hand reaches neither, and configure is the one point every route
passes through. The rule is the three-valued one
`scripts/lib/checkout-paths.sh` already states for inherited SDK paths (issue
1280): an application outside any nano-ros checkout is accepted (a copied-out
example, a downstream project), an application inside the module's own checkout
is accepted, and an application inside a DIFFERENT checkout is refused naming
both. Ownership is a lexical walk for `packages/core/nros-core/Cargo.toml`,
never `.git` — a linked worktree's `.git` is a file (issue 1336), and linked
worktrees are the population this defect was found in.

**4. A gate, `check-zephyr-module-binding`** (fast lane), because the failure
mode is a wrong image that still links. It harvests every `west build` in
`just/`, `scripts/`, `tests/` and `ci/`, skips `-t <target>` re-entries, and
requires the rest to take the flag from the helper — a hand-written
`-DZEPHYR_EXTRA_MODULES=` fails, which is what keeps "one helper" true. It runs
its own negative control on every invocation.

### Measured

* **A real image, built from this worktree against the still-BOUND store
  workspace** (the symlink was deliberately not repaired, so this measures the
  fix and not the host):
  `just zephyr build-one c/talker zenoh` → `zephyr.elf`, with
  `[1291/1296] Building C object modules/nros/…/home/aeon/nros-agent-worktrees/1379/packages/rmw/zenoh/zpico-sys/c/zpico/zpico.c.obj`
  — the same file that came from the other clone in the report. In the new
  build directory, `"nros":"/home/aeon/nros-agent-worktrees/1379"`,
  `NROS_REPO_DIR` is this worktree, and `grep -c simple-autoware-safety-island
  build.ninja` is **0**.
* **The guard refuses in situ**, inside a real Zephyr configure: the talker
  copied into a synthetic second checkout, built with this worktree as the
  module, dies at `nano-ros: this image would mix TWO nano-ros checkouts
  (issue 1379)` naming both trees. Run entirely in scratch space — no second
  real checkout was configured against.
* **The guard's four rows** exercised directly with `cmake -P`: same checkout
  ACCEPTED, application outside any checkout ACCEPTED, foreign checkout
  REFUSED, `-DNROS_ALLOW_FOREIGN_MODULE=ON` downgraded to a warning.
* **The gate's mutation**, at both shapes it has to handle: deleting
  `"$nros_module_arg"` from `just/zephyr-dev.just` turns it red naming
  `just/zephyr-dev.just:148`, and deleting the `replace_or_append_arg` line
  from `scripts/build/zephyr-fixture-run-one.sh` — where the flags are
  assembled into an array thirty lines before the `west build` — turns it red
  naming line 278. Restoring each turns it green at 9 harvested invocations.

  The array shape is worth recording because the gate's first version could
  not see that file at all: it walked the filesystem and pruned directories
  named `build`, which is `scripts/build/`. Converting it to the git index
  (`scripts/lib/tracked.py`, required by `check-no-tracked-file-find`) both
  removed the walk and closed the hole — the gate was reporting OK over a set
  that excluded the tree's busiest west builder.
* `just check fast`: **326 gates ran, 2 skipped (unprovisioned FSP tree,
  unsynced nuttx-ffi leaves), 0 failed.**

### What was NOT measured, and what was deliberately not changed

* **The 4.4 line.** Everything above is the 3.7 line. The 4.4 store workspace
  carries the identical symlink, and the recipes are shared, so the fix applies
  by construction — but no 4.4 image was built.
* **The FVP recipes** (`build-fvp-ws-entry`, `build-fvp-board-import`) gained
  the flag and were not run: they need an AEMv8-R FVP model this host does not
  have. Their `west build` lines are covered by the gate, not by a build.
* **The build directory is still named from the leaf alone**, and this is a
  decision rather than an omission. West's own `_sanity_check` compares the
  cached `APPLICATION_SOURCE_DIR` against the one being built and, at the
  default `pristine=never`, refuses ("please clean it, use --pristine, or use
  --build-dir"); two checkouts building one leaf therefore collide LOUDLY, not
  silently. Renaming the directory would move it out of the vocabulary
  `check-west-leaf-vocabulary` models and the fixture resolvers read, where a
  name no lane knows produces a STALE verdict indistinguishable from a cell
  that ran and failed (issue 1016) — a silent failure traded for a loud one.
* **Two checkouts building the same leaf CONCURRENTLY into one store build
  directory is still unguarded** — nothing locks it, and west's sanity check
  only compares a cache that the other build may not have written yet. Filed
  separately as issue 1399; it is a race, not the wrong-artifact defect this
  issue reports.
* **The workspace on this host is still BOUND.** `scripts/zephyr/unbind-manifest-project.sh`
  (phase-449 W1) repairs it in place and was deliberately not run, so that the
  measurement above is about the fix rather than about the host. Anyone who
  wants the second layer of protection should run it.
