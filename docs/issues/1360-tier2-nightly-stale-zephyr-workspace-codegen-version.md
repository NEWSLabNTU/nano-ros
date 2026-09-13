---
id: 1360
title: "`tier 2 nightly` builds against a PERSISTENT `~/.nros/workspaces/zephyr/3.7`
  whose generated trees were emitted at an older codegen version, so the refusal guard
  fires on every fixture — the guard is right and the workspace is the bug"
status: open
type: bug
area: ci, codegen
severity: medium
found: 2026-09-13
related: [1018, 1115, 1158, 0466]
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
