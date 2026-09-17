---
id: 1360
title: "`tier 2 nightly` builds against a PERSISTENT `~/.nros/workspaces/zephyr/3.7`
  whose generated trees were emitted at an older codegen version, so the refusal guard
  fires on every fixture — the guard is right and the workspace is the bug"
status: resolved
type: bug
area: ci, codegen
severity: medium
found: 2026-09-13
resolved: 2026-09-18
related: [1018, 1115, 1158, 0466, 1253, 1280, 0196, 1226, rfc-0090, rfc-0095]
---

## What happens

Nightly run **34739657223** (schedule, 2026-09-13T05:10), job **103677080186**
(`tier 2 nightly (pairwise cover)`), step `just build tier2-nightly`. The Zephyr
fixture build fails, and the first error lines name the cause:

```
== zephyr == FAILED (rc=2)
first error line(s) in tmp/build-test-fixtures-…/zephyr.log:
  /home/runner/.nros/workspaces/zephyr/3.7/build-cortex-m-c-talker-zenoh/nano_ros_c/
    std_msgs/msg/std_msgs_msg_string.h:24:2: error: #error "nros: this generated tree
    was emitted at a codegen version the runtime does not accept
    (see NROS_EMITTED_CODEGEN_VERSION …)"
  FATAL ERROR: command exited with status 1: cmake --build
    /home/runner/.nros/workspaces/zephyr/3.7/build-cortex-m-c-talker-zenoh
error: recipe `build-fixtures` failed with exit code 2
```

The same `#error` appears for `build-c-service-client-xrce`
(`unique_identifier_msgs`), and a separate leaf fails at link
(`collect2: error: ld returned 1 exit status`), which is the same staleness one
layer down.

The path is the finding: `/home/runner/.nros/workspaces/…`, not the job's checkout.
That workspace **persists across runs** on this runner, so it holds generated trees
emitted by an older `nros`. The guard added for the codegen-version pair does exactly
what it exists to do — it refuses a museum tree rather than compiling it — and what it
caught is the workspace, not the code.

## Why it matters

Tier 2's pairwise cover is where the platform×language and rmw×language classes
surface (issues 0268/0245, 0332, 0331). While the workspace is stale the lane cannot
reach a single cell, so that whole cover is unmeasured, and the lane reports the same
`failure` whether or not anything is actually broken.

## What this is NOT

- **Not issue 1359.** That is the 22 `zephyr *` jobs of the same run dying in
  `_setup-common` for a missing `unzip`. This job gets much further — it provisions,
  builds, and fails on the content of a workspace.
- **Not a `rm -rf` problem to be solved by `rm -rf`.** CLAUDE.md is explicit that
  wiping destroys the reproduction and teaches the next person to distrust the tree.
  The generated trees here are byproducts of a CONFIGURE-time emitter, which is the
  shape issue 1018 addressed with `nros_codegen_tool_reconfigure()` — the question is
  why that edge does not reach this workspace.
- **Not issue 1115.** That is NuttX's committed snapshot header; this is a Zephyr west
  workspace's per-build generated tree.

## What would close it

1. Establish why the persistent workspace's generated trees are not re-emitted when
   the `nros` CLI moves. `source_stamp` and `CMAKE_CONFIGURE_DEPENDS` are supposed to
   make a CLI rebuild re-stale exactly this (issues 0466, 1018) — so either the edge
   does not reach a west workspace, or the nightly reuses the workspace without
   re-running the configure that would notice.
2. Make the failure name itself: the `#error` says the tree is stale but not which
   `nros` emitted it or which the runtime wants. Printing both versions turns this
   from an archaeology session into a line of output.
3. Decide the lane's contract for a persistent workspace — refresh it when the CLI
   stamp changes, or treat it as a cache that must be invalidated on that key.

Acceptance is `tier 2 nightly` reaching its cells — a verdict on the pairwise cover,
green or red — rather than failing in `build-fixtures`.

## The same cause in a second lane: `run-matrix.yml` tier 2

Run **34742879689** (schedule, 2026-09-13T06:29), job **103685468305**
(`tier 2 (1-wise matrix)`), step `just build tier2`:

```
/home/runner/.nros/workspaces/zephyr/3.7/build-cpp-talker-xrce/nano_ros_cpp/std_msgs/msg/
  std_msgs_msg_uint8.hpp:28:2: error: #error "nros: this generated tree was emitted at a
  codegen version the runtime does not accept …"
error: recipe `build-fixtures` failed with exit code 2
```

Same persistent workspace, same guard, same `build-fixtures` failure — so this is
not only the nightly pairwise cover. The 1-wise lane has failed on 2026-09-11, -12
and -13.

Worth recording because of what it changes about triage: this lane's reds were
being read as issue 1158 (tier 2 never reaching its cells, by provisioning or
build stage). The stage is right — it never reaches the cells — but the cause is
this workspace, not 1158's. A stage axis says how far a lane got; it does not say
why, and reading one as the other is how a second cause hides behind a chronic
red.

## Resolution (2026-09-18)

### Question 1 — why the persistent workspace is not re-emitted when `nros` moves

The issue asked whether `source_stamp` / `CMAKE_CONFIGURE_DEPENDS` (issues 0466,
1018) fail to reach a west workspace, or whether the nightly reuses it without a
configure. Neither. **Both edges fire; they name the wrong binary.**

Every freshness input the Zephyr interfaces emitter has is keyed on
`_NROS_ZEPHYR_CODEGEN_TOOL` — the `CMAKE_CONFIGURE_DEPENDS` registration issue
1018 added, and the `IS_NEWER_THAN` loop it made reachable. That variable is
`CACHE INTERNAL`, and `_nros_resolve_codegen_tool()` drops a cached value only
when the path stops **existing**. So a build directory keeps whichever `nros`
first configured it, for as long as that file is there.

RFC-0095 D4 then moved the workspace to `$NROS_STORE/workspaces/zephyr/<version>`
— **outside every checkout**, which is the point of the store and also what makes
its build dirs outlive any one clone. On the self-hosted runner two checkouts are
in play:

* `/home/runner/src/nano-ros` — `runner-bootstrap.sh`'s shallow clone, pinned at
  the bootstrap `REF`. `runner-provision.sh` runs `just setup zephyr` from it, so
  **it** configures the store's build dirs and its `packages/cli/target/release/nros`
  is what they cache.
* `/home/runner/_work/nano-ros/nano-ros` — the job checkout, `git clean -ffdx`'d
  and rebuilt every run (run 34739657223 logs `[setup-cli] built: …/_work/…` at
  05:11:27).

The job never rebuilds the first one, so its mtime never advances: the
configure-depends edge is satisfied, the `IS_NEWER_THAN` loop is satisfied, and
the trees are never re-emitted. Both checkouts being live in one build dir is
visible in the failing run's own log —

```
[1305/1315] Building C object modules/nros/CMakeFiles/nros.dir/
  home/runner/src/nano-ros/packages/rmw/zenoh/zpico-sys/c/zpico/zpico.c.obj
```

— a C TU from the provisioning checkout, in the same step whose cargo half is
compiling `…/_work/nano-ros/nano-ros/packages/core/nros-core`.

**Which side of the range was violated: the tree was too OLD.** The two
directions produce the same `#error`, so it was settled on mechanism rather than
on the message. `nros_config_generated.h` — the runtime half — is a cargo
build-script byproduct of `nros-c`, and a fresh checkout gives every source a new
mtime, so cargo rewrites it at this tree's version on every run. The generated
message trees are a configure-time byproduct whose only trigger is the cached
tool. One side moves reliably and the other cannot: `emitted < NROS_CODEGEN_VERSION_MIN`.
`MIN` went 1 → 2 on 2026-09-05, which is what invalidated trees emitted in the
version-1 window; before that the lane was failing earlier (issue 1158 measured 5
provisioning / 3 build, 0 reaching cells), which is why the first build-stage red
appears on 2026-09-11.

Corroborating, not decisive: the failing headers report the `#error` at
`std_msgs_msg_string.h:24` (C) and `std_msgs_msg_uint8.hpp:28` (C++), and
regenerating both locally with this tree's `nros` puts it at exactly lines 24 and
28 — the layout matches, so the guard block is the one the current packs emit.

### The fix — the emitted version is a freshness input

A generated header **states** the version it was emitted at
(`#define NROS_EMITTED_CODEGEN_VERSION`, from `packs/_codegen_version.jinja`) and
this tree **states** the range it accepts. Comparing the two is a pure function of
two files on disk: independent of mtimes, of which binary a directory cached, and
of which checkout provisioned it. Three functions in the shared core
(`cmake/NanoRosCodegenCore.cmake`):

* `nros_codegen_accepted_range(<min> <max>)` — parses
  `packages/core/nros-core/src/codegen_version.rs`, located from this module's own
  `CMAKE_CURRENT_LIST_DIR` so it is the checkout DRIVING the configure and not an
  inherited variable (issue 1280's class). Empty when unreadable → note-and-continue,
  `abi_guard`'s contract. **Not** `nros --codegen-version`: a stale tool reports a
  stale version, agrees with the stale tree it emitted, and reports FRESH.
* `nros_codegen_version_stale(<out> [REJECTED <var>] [CONTEXT <l>] FILES …)` — one
  `message(STATUS)` per refused file naming the emitted version, the range and the
  tree the range came from. That is the issue's point 2: the `#error` can name
  macros but never their values.
* `nros_codegen_version_assert_fresh(<tool> …)` — called after a regeneration. A
  tree still refused means the cached tool is not this runtime's emitter, so it is
  a configure `FATAL_ERROR` naming the binary and both numbers, instead of the same
  `#error` fifteen minutes later inside a museum header.

Wired into **both** generators, because both key on a cached tool:
`zephyr/cmake/nros_generate_interfaces.cmake` (configure-time) sets its existing
`_codegen_needed` flag and then asserts; `cmake/NanoRosGenerateInterfaces.cmake`
(build-time) removes exactly the refused outputs, which is the input its own
`add_custom_command` is driven by. No wipe: the removal is per-file, logged with
both versions, and the re-emit is done by the tool that owns the tree.

The range is read on **every** call and never cached across configures. The first
draft of `nros_codegen_accepted_range` stashed it in a `CACHE INTERNAL` pair —
this issue's own defect one layer up, since a persistent build dir would then
answer with the range of whichever checkout configured it and a `MIN` bump would
be invisible to the check that exists to notice it. Measured with the read
uncached: two configures of one build dir against a tree whose `MIN` moves 4 → 6
report `[4, 5]` then `[6, 5]`, and a version-5 header goes from accepted to
refused. The reader also registers `codegen_version.rs` in
`CMAKE_CONFIGURE_DEPENDS`, so a bump re-runs the configure on its own rather than
waiting for a tool rebuild to do it.

The lane's contract for a persistent workspace (point 3) is therefore **refresh it
on that key** — a version bump regenerates, and only a genuinely foreign emitter
still stops the build, loudly and at once.

### The gate — one existing gate widened at each level, per the 0196 rule

* `check-codegen-tool-reconfigure` (fast line) gains **rule 2**: a file that calls
  `_nros_predict_generated_outputs()` — the shared "here is what codegen will
  emit", so the owner of a version-stamped tree — must also call
  `nros_codegen_version_stale()`. Its rule was always "an emitter must state every
  edge that can leave it museum code"; the tool edge was the only edge it knew.
  7 new selftest cases, including that registering the TOOL does not satisfy the
  version rule. Verified against the real files: reverting the Zephyr arm reports
  `['zephyr/cmake/nros_generate_interfaces.cmake']`, as shipped reports `[]`.
* `check-codegen-version-refusal` gains **arm E**, the freshness half. Arms A–D
  prove the guard fires; E proves the build regenerates rather than letting it,
  which had no evidence at all. It drives the canonical generator through a real
  configure + build in a scratch project, poisons the stamp to 1, asserts an
  incremental build does **not** notice (the reproduction), then asserts the
  re-configure names both numbers and the rebuild restores the version.

### Evidence

Arm E is the measured acceptance, and it is a BUILD:

```
  ok    E  a clean configure+build emits at this tree's version (5)
  ok    E  reproduction: an incremental build does NOT notice (no input moved)
  ok    E  a re-configure names the refused version AND the accepted range
  ok    E  the tree is REGENERATED at 5 — the guard never fires
```

with the reproduction's own compile failure being this issue's exact text
(`error: #error "nros: this generated tree was emitted at a codegen version the
runtime does not accept …"`), and the repair line reading

```
-- nros codegen (std_msgs, C): regenerating — …/std_msgs_msg_string.h was emitted
   at codegen version 1, and this runtime accepts 2..5
   (…/packages/core/nros-core/src/codegen_version.rs)
```

Negative control: with the canonical lane's arm reverted, arm E fails on
`the museum tree survived the re-configure (stamp 1)`.

`check-codegen-version-refusal` and `check-codegen-tool-reconfigure` both run on
the **fast** line, which `build-test-fixtures` depends on — so the widened gates
run inside the very step that failed, on every push and every PR.

**Not reproduced end to end:** the real CI condition needs a west workspace in the
store configured by a second checkout, and this host has no store workspace (the
only Zephyr tree is the shared checkout-relative one, which other sessions build
in). The Zephyr arm is the same two calls on the same helper as the canonical arm,
and the file parses and defines its generator cleanly; the helper itself is
covered directly (range parse, per-file stamp read, unstamped umbrella headers
skipped, both range directions, the `assert_fresh` FATAL).

### What is deliberately NOT changed

* **The `#error` text still cannot name the versions.** It could carry the emitted
  one as a literal (codegen knows it), but that is a pack edit that moves the
  codegen fingerprint and rewrites ten golden fixtures, for a message the fix makes
  unreachable in this condition — and the value is already three lines above it in
  the same file. The configure-time lines above carry both numbers.
* **A stale `nros_config_generated.h`** (the other direction: a correct tree against
  an old runtime header) is not addressed here. Regeneration cannot fix it, its
  stale side is a cargo byproduct that is absent or previous-run at configure time,
  and reading it as the authority would turn "the tree is stale" back into
  "something is stale". That is the 0088/0834 mirror class.
* **`nros sync`'s Rust `generated/` trees.** `abi_guard::check_workspace` already
  refuses a stale CLI on the `Sync` verb with an actionable error, and the measured
  failure is the two cmake lanes.
* **Where the runner's workspace comes from.** `check-zephyr-workspace-checkout.sh`
  (issue 1253) already asks whether a provisioned workspace belongs to this
  checkout, and its west-manifest arm covers exactly this runner shape — but it is
  reached only from `check-tier-preconditions`, i.e. from `just ci`, one step AFTER
  `just build`. That is issue 1226's shape and worth its own change; it is not this
  one, because with the version arm in place the workspace now repairs itself
  rather than needing to be refused.
