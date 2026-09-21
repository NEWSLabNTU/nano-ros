---
id: 1389
title: "tier 2 dies configuring a persistent Zephyr workspace: the entity-inventory
  fragment states schema 6 and the READER answers 3, though producer and reader are
  both 6 on `main` — 1360's class with the direction reversed and a different artifact"
status: resolved
type: bug
area: [ci, build, codegen]
severity: medium
found: 2026-09-18
related: [1360, 1158, 1280, 1253, 0196]
---

## What happens

`run-matrix.yml` tier 2 (the 1-wise cover, 06:28 UTC) reaches the fixture build
and dies in **cmake configure** of a leaf, against the persistent workspace under
`~/.nros/workspaces/zephyr/3.7/`:

```
nros:
/home/runner/.nros/workspaces/zephyr/3.7/build-cortex-m-c-talker-zenoh/nros/entity_inventory.cmake
states entity-inventory schema version 6; this reader understands 3.
  Refusing rather than reading fields that may have moved.
  Rebuild the `nros` CLI so the producer and the reader come from one tree
Call Stack (most recent call first):
  .../cmake/NanoRosNodeRegister.cmake:173 (nros_derive_entity_inventory_knobs)
  .../cmake/NanoRosNodeRegister.cmake:150 (_nros_node_register_compose_inventory)
  CMakeLists.txt:DEFERRED
```

The guard is right to refuse. What is wrong is the number it compared against.

## Evidence

| what | value |
| --- | --- |
| run | 35315010126 (`run-matrix`, schedule, 2026-09-18T06:28:12Z) |
| head | `3a3ec205` |
| job | 105504675372 `tier 2 (1-wise matrix)`, step `just build tier2` |
| reporting job | 105509163214 — `tier 2 — NO VERDICT: stopped in the build` |
| leaf | `build-cortex-m-c-talker-zenoh` |

**The two numbers agree on `main`.** At `3a3ec205`:

* producer — `packages/cli/nros-cli-core/src/entity_inventory.rs:188`,
  `pub const ENTITY_INVENTORY_SCHEMA_VERSION: u32 = 6;`
* reader — `cmake/NanoRosEntityInventory.cmake:256`,
  `set(NROS_ENTITY_INVENTORY_SCHEMA_SUPPORTED 6 CACHE INTERNAL ...)`

So the artifact's `6` is what this tree's CLI emits, and the `3` the reader
answered with is **not a value that exists anywhere in the checkout that drove
the configure**. The fragment is current; the reader is stale.

That points at the storage rather than at either number: the reader is a
`CACHE INTERNAL` variable, so its value lives in the persistent build
directory's `CMakeCache.txt`, not in the module — and the build directory is
reused across runs and across the checkouts that configured it.

**This is exactly the hazard issue 1360 named and then removed from its own
reader.** 1360's resolution says of its first draft:

> The first draft of `nros_codegen_accepted_range` stashed it in a
> `CACHE INTERNAL` pair — this issue's own defect one layer up, since a
> persistent build dir would then answer with the range of whichever checkout
> configured it and a `MIN` bump would be invisible to the check that exists to
> notice it.

1360 fixed that for the codegen-version range (read on every call, never cached,
located from the module's own `CMAKE_CURRENT_LIST_DIR`). The entity-inventory
schema reader is the sibling that still has it.

## What this is NOT

* **Not 1360.** That is `NROS_EMITTED_CODEGEN_VERSION` on generated headers,
  resolved 2026-09-18 at `3fe1ef266`. Different artifact, different guard, and
  its fix does not read this variable. This failure appeared on the first tier-2
  run after that fix landed, which is consistent with 1360 having removed the
  error that used to mask it.
* **Not the opposite direction.** 1360 is a STALE artifact refused by a current
  tree. Here the artifact is CURRENT (schema 6, what today's CLI emits) and the
  reader is BEHIND (3). "Rebuild the `nros` CLI", which the message advises,
  would not change either number.
* **Not 1158.** The lane reached the build stage; the stage reporter says so
  (`stopped in the build`). This is not a provisioning failure.
* **Not a stale `nros` binary.** The fragment carries the version today's
  producer writes.
* ~~**Not 1253.**~~ **It IS 1253** — see "Step 1, answered" below. This entry
  was written before the call stack was read and is wrong; it is struck through
  rather than deleted because it is the reason nobody looked at the manifest
  binding for three more runs.

## What would close it

Measurement first — the exact provenance of the `3` is not established, and
there is more than one defensible fix site:

1. **Establish where the `3` comes from.** Print
   `NROS_ENTITY_INVENTORY_SCHEMA_SUPPORTED` and its source at configure time in
   that leaf, and read the persistent build dir's `CMakeCache.txt` for the
   variable. Either it is cached from an older configure of that directory, or
   an older module is on the include path ahead of the checkout's. The two have
   different fixes and the message cannot currently tell them apart.
2. **If it is the cache**: give the reader 1360's treatment — read the supported
   version on every call from the module's own `CMAKE_CURRENT_LIST_DIR`, never
   from a `CACHE INTERNAL` slot, and register the defining file in
   `CMAKE_CONFIGURE_DEPENDS` so a bump re-runs the configure. Per the 0196 rule,
   check whether the same shape covers `cmake/NanoRosMessageBounds.cmake`, which
   reads the same variable at `:787` and `:864`.
3. **Make the message name its own provenance.** It states both numbers but not
   where either came from; 1360's point 2 is the same complaint one artifact
   over. A reader that printed the file it read the supported version from would
   have answered question 1 without an investigation.
4. **Decide the lane's contract for the persistent workspace** — the same open
   question 1360 answered for generated trees (refresh when the producer moves).
   A cached reader variable is the second thing in that directory that outlives
   the tree that wrote it.

A gate is only worth designing after step 1: whether the right check is
"no schema reader may be `CACHE INTERNAL`" or something narrower depends on what
the provenance turns out to be.

## Step 1, answered (2026-09-20) — an older module, not the cache

Run 35517366695 (`workflow_dispatch`, 14:43 UTC) reproduces it, and the call
stack in that run settles the question the issue could not:

```
CMake Error at /home/runner/src/nano-ros/cmake/NanoRosEntityInventory.cmake:360 (message):
  nros:
  /home/runner/.nros/workspaces/zephyr/3.7/build-cortex-m-c-talker-zenoh/nros/entity_inventory.cmake
  states entity-inventory schema version 6; this reader understands 3.
Call Stack (most recent call first):
  /home/runner/_work/nano-ros/nano-ros/cmake/NanoRosNodeRegister.cmake:173
  /home/runner/_work/nano-ros/nano-ros/cmake/NanoRosVerbs.cmake:82
```

**Two checkouts, one configure.** `NanoRosNodeRegister.cmake` and
`NanoRosVerbs.cmake` resolve under `/home/runner/_work/nano-ros/nano-ros` — the
job's checkout. `NanoRosEntityInventory.cmake` resolves under
`/home/runner/src/nano-ros` — the runner's own provisioning checkout. The `3`
is not a cached value that went stale; it is simply what that older tree's
module file says, read out of a file this checkout does not contain.

That makes hypothesis 2 of step 1 the answer and hypothesis 1 wrong, so fix
sites 2 and 4 — the `CACHE INTERNAL` treatment and a contract for cached
variables — address a defect that is not here. Fix site 3 stands on its own
merits and would have answered this in one run: a message that named the file
it read the supported version from makes "which checkout is this?" unmissable.

### Why `ZEPHYR_EXTRA_MODULES` did not rescue it

phase-449 W1 added `-DZEPHYR_EXTRA_MODULES=<checkout>` to the fixture runner,
and the failing command line carries it:

```
-DZEPHYR_EXTRA_MODULES=/home/runner/_work/nano-ros/nano-ros
```

It loses anyway. West's manifest project ALREADY provides a module named
`nros`, and an extra module does not displace one the manifest supplies. So the
flag is necessary but not sufficient: the manifest project has to stop being a
checkout at all, which is what `unbind-manifest-project.sh` does.

### Why it survived W1

W1 removed the binding from `scripts/zephyr/setup.sh` and made that script
unbind — but `just zephyr setup` does not call it when the workspace already
exists, and every existing workspace was provisioned before W1. So a host that
had one kept it, and `--force` (a full reprovision) was the only repair.
`~/.nros/workspaces/zephyr/3.7/` on the self-hosted runner is one of those. The
same gap that issue 1279 found for SDK registration, one rung over, in the same
`if` statement.

### Fixed by

* `just/zephyr-setup.just` — the workspace-already-present branch now calls
  `unbind-manifest-project.sh --from-config "$WORKSPACE"`. `--from-config`
  because the project directory is named after the basename of whichever
  checkout ran `west init -l`, which for a shared store workspace is not the
  caller's to assume.
* `.github/workflows/run-matrix.yml` — `check-zephyr-workspace-checkout.sh`
  moved ahead of `just build tier2`. It already ran inside `just ci matrix`,
  which is the step AFTER the build it needed to prevent.

Measured end to end on a synthetic workspace with the runner's exact shape (a
`.west/config` whose `[manifest] path` is a symlink to a foreign checkout
carrying `zephyr/module.yml`, `packages/core/nros-core/Cargo.toml` and
`packages/cli/Cargo.toml`): the gate refuses and names all three paths, the
unbind repairs it, and the gate then passes. The gate is also idempotent on an
already-unbound workspace.

## Resolution

Two halves, landed separately. The ROOT CAUSE is the one "Fixed by" above
records — the workspace's west manifest project was a foreign checkout, and
`just/zephyr-setup.just` now unbinds it while
`check-zephyr-workspace-checkout.sh` runs ahead of `just build tier2`.

This section closes fix site 3, which the issue said "stands on its own merits
and would have answered this in one run": **the refusal could not name itself.**

### The mechanism, measured

Two things here read like they cannot happen, so both were measured rather than
argued. The probes are in `tmp/1389/` (gitignored), never in `~/.nros`.

**1. Two copies of one module both run, and the last one wins.**
`include_guard(GLOBAL)` keys on the RESOLVED PATH, so two checkouts' copies are
two guards. Reproduced with the runner's include order — the job checkout's
copy (schema 6) first, then the manifest project's (schema 3):

```
-- PROBE: the live reader answers 3
CMake Error at .../twocheckout/old/cmake/Reader.cmake:11 (message):
  nros: .../build/fragment.cmake
  states entity-inventory schema version 6; this reader understands 3.
```

That is the runner's text verbatim, from a fragment stating exactly what
today's CLI emits. The `function()` definitions move with the constant: the
reader that ANSWERS is whichever copy ran last, which on a west build is the
manifest project's.

**2. The `CACHE INTERNAL` hypothesis is DISPROVEN.** The issue's step 1 said
the cache was not the cause; this measures WHY it could not have been.
`set(<x> <v> CACHE INTERNAL ...)` implies FORCE, so a persistent build dir
cannot hold a stale reader. Configure a build dir with the module at 3, bump
the module literal to 6, re-configure the SAME dir:

```
-- PROBE: reader answers 3
NROS_PROBE_SCHEMA_SUPPORTED:INTERNAL=3
    (module bumped 3 -> 6)
-- PROBE: reader answers 6
NROS_PROBE_SCHEMA_SUPPORTED:INTERNAL=6
```

So the fix is NOT "stop caching a module constant". That storage is this tree's
deliberate idiom at ~100 sites for the `_NROS_ENTRY_DIR` reason (these files are
reachable from inside a function frame, where a file-scope plain `set()` dies
with the frame while `include_guard` makes every later include a no-op).
Stripping it would reintroduce 287-W6 across the tree to fix a staleness that
does not exist. Sweep that established this:

```
grep -rn 'CACHE INTERNAL' cmake/ zephyr/ --include='*.cmake' --include='CMakeLists.txt'
```

Every one of those sites stores a module DIRECTORY, a discovered fact, or a
memo — none is a value a persistent build dir can serve stale.

### What changed

`cmake/NanoRosSchemaReader.cmake` (new) holds one
`nros_schema_mismatch_diagnosis()`. A refusal has three facts and used two; the
third is the one that separates "the producer is stale" from "two nano-ros trees
reached one configure":

1. the artifact's version and path — it had these;
2. the reader's version — it had this;
3. the FILE the reader's version came from — it did not.

Every declared `NROS_*_SCHEMA_SUPPORTED` now records its own file in a `_FROM`
partner. The helper module sets no file-scope variable of its own, only a
function, so it is immune to the frame-pop hazard by construction.

The DIRECTION is a diagnosis on its own, and the old text ignored it by giving
one piece of advice for both cases. Artifact NEWER than reader ⇒ the READER is
behind, and "rebuild the `nros` CLI" is definitionally wrong — a newer CLI
cannot have written an older fragment. Artifact OLDER ⇒ the artifact is stale,
which is the only case the old advice fit.

Sweep — three refusal sites cite a SUPPORTED constant, all three fixed together
(`grep -rn 'SCHEMA_SUPPORTED' cmake/ zephyr/ --include='*.cmake'`):

* `NanoRosEntityInventory.cmake` entity refusal (FATAL)
* `NanoRosMessageBounds.cmake` message-bound refusal (FATAL)
* `NanoRosMessageBounds.cmake` payload-class join (WARNING, deliberately not
  fatal) — the one that reads the OTHER module's constant, so "which copy
  answered" is live there even within one checkout.

### A defect the measurement caught and reading would not

`if(<unset-var> STREQUAL "")` is FALSE, not true: cmake dereferences the left
operand only when it names a DEFINED variable, and otherwise compares the
literal string `_a_ARTIFACT_VERSION`. The first draft therefore reported "the
artifact is OLDER" for a fragment stating no version at all — on a LIVE path,
since the payload-class join reaches the diagnosis with an undefined version on
purpose (`NOT DEFINED ... OR NOT ... EQUAL ...`). Now `NOT DEFINED` is tested
first and the numeric shape is checked before comparing.

### Before and after

Same reproduction, same two checkouts. Before:

```
  nros: .../build/fragment.cmake
  states entity-inventory schema version 6; this reader understands 3.

    Refusing rather than reading fields that may have moved.
    Rebuild the `nros` CLI so the producer and the reader come from one tree: ...
```

After (the `nros_schema_mismatch_diagnosis()` that ships, copied into each
synthetic checkout as a checkout would carry it):

```
  nros: .../build/fragment.cmake
  states entity-inventory schema version 6; this reader understands 3.

    Refusing rather than reading fields that may have moved.
    This reader's number comes from:
      .../twocheckout/old/cmake/Reader.cmake
    The artifact is NEWER than this reader, so the READER is behind and
    rebuilding the `nros` CLI would move neither number.
    If the file named above is not inside the checkout you are building,
    TWO nano-ros trees have reached one configure and the older one
    answered: `include_guard(GLOBAL)` keys on the resolved path, so both
    copies run and the last one wins. For a persistent Zephyr workspace
    that is usually its west MANIFEST PROJECT -- check it with
    `scripts/check-zephyr-workspace-checkout.sh` and repair it with
    `scripts/zephyr/unbind-manifest-project.sh --from-config <workspace>`.
    Otherwise this checkout's module file is genuinely older than its CLI.
```

### Reproduced on a DEVELOPER host, unplanned, with the real modules

The two-checkout shape is not CI-only. Configuring
`examples/native/c/listener` from a nested agent worktree reproduces it exactly,
because module resolution picks a prefix rather than "the checkout I am in":

```
$ cmake -S examples/native/c/listener -B <build>
$ grep -E 'SCHEMA_SUPPORTED|_NANO_ROS_PREFIX' <build>/CMakeCache.txt
NROS_ENTITY_INVENTORY_SCHEMA_SUPPORTED:INTERNAL=3
NROS_MESSAGE_BOUNDS_SCHEMA_SUPPORTED:INTERNAL=1
_NANO_ROS_PREFIX:INTERNAL=/home/aeon/repos/nano-ros          <-- the PARENT checkout
nano_ros_DIR:PATH=/home/aeon/repos/nano-ros
```

**That is the issue's `3`, on a laptop.** The parent checkout sits on another
branch whose `cmake/NanoRosEntityInventory.cmake:183` still says
`set(NROS_ENTITY_INVENTORY_SCHEMA_SUPPORTED 3 ...)`, and the worktree's own
module — the one being edited, stating 6 — never loaded. Same configure with
the prefix pointed at the worktree:

```
NROS_ENTITY_INVENTORY_SCHEMA_SUPPORTED:INTERNAL=6
NROS_ENTITY_INVENTORY_SCHEMA_SUPPORTED_FROM:INTERNAL=<worktree>/cmake/NanoRosEntityInventory.cmake
NROS_MESSAGE_BOUNDS_SCHEMA_SUPPORTED_FROM:INTERNAL=<worktree>/cmake/NanoRosMessageBounds.cmake
```

Both configures exit 0 — this leaf reaches `nano_ros_add_executable` only, so
it never calls the entity reader and never compares. The point is the FACT the
cache now carries: had it compared, the refusal would have named
`/home/aeon/repos/nano-ros/cmake/NanoRosEntityInventory.cmake` and the answer
would have been one line instead of three CI runs. This is the same class as
issues 1280/1391 (an inherited absolute path outranks the checkout you are
building) reaching a cmake module rather than an SDK path.

### The tier-2 failure itself, re-rendered through the SHIPPED module

Driving `cmake/NanoRosEntityInventory.cmake` in `cmake -P` script mode with the
runner's own numbers (fragment 6, reader 3) and the runner's own provenance:

```
  nros: /home/runner/.nros/workspaces/zephyr/3.7/build-cortex-m-c-talker-zenoh/nros/entity_inventory.cmake
  states entity-inventory schema version 6; this reader understands 3.

    Refusing rather than reading fields that may have moved.
    This reader's number comes from:
      /home/runner/src/nano-ros/cmake/NanoRosEntityInventory.cmake
    The artifact is NEWER than this reader, so the READER is behind and
    rebuilding the `nros` CLI would move neither number.
    ...
```

`/home/runner/src/nano-ros` is the runner's provisioning checkout. Establishing
that took three runs, a `workflow_dispatch` re-run and a call-stack read; the
refusal now states it in the line under the numbers.

### Gate

`check-schema-reader-provenance` (`just check schema-reader-provenance`;
`scripts/check-schema-reader-provenance.py`). Buildless, self-testing on every
normal run, 16 negative controls. Two rules — a declared constant records its
file, and a version comparison reaches the shared diagnosis.

REACH is every tracked cmake file outside `third-party/` (361 of them), not
just the two that declare a constant today, per the 0196 rule. Verified with
planted violations against the REAL file set: dropping the `_FROM` from
`NanoRosEntityInventory.cmake`, replacing the helper call in
`NanoRosMessageBounds.cmake`, and a brand-new reader at
`cmake/board/_planted.cmake` are each caught, on both rules.

### What this does NOT close

* It does not provision, clean or repair the runner's persistent workspace under
  `~/.nros/workspaces/zephyr/3.7/`. An existing workspace provisioned before
  phase-449 W1 still carries the foreign manifest binding until
  `just zephyr setup` re-runs the unbind on it (or `--force` reprovisions); the
  gate ahead of `just build tier2` is what makes that visible rather than fatal
  mid-build.
* It does not make two checkouts in one configure safe — only legible. The
  refusal now names the file that answered; it cannot choose the right copy.
* Nothing here changes any schema NUMBER, on either side.
