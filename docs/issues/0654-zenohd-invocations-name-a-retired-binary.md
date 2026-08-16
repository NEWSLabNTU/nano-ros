---
id: 654
title: "Every `zenohd --listen …` in the tree names a binary that no longer exists and passes flags the replacement ignores — ~95 files, two of them executable"
status: open
type: bug
severity: medium
area: docs, scripts, examples
related: [issue-0653, issue-0374, rfc-0075, phase-362]
---

## What is wrong

[phase-362](../roadmap/archived/phase-362-zenoh-router-from-ros-not-vendored.md)
retired the vendored `zenohd` and made the router
`rmw_zenoh_cpp/rmw_zenohd`, resolved out of `/opt/ros`. The retirement covered
the submodule, `just/zenohd.just`, the SDK index entry and the test harness. It
did not cover the ~95 files that tell a reader to run the old binary.

Those instructions are wrong in **two independent ways**, and the second is the
nastier one:

1. **The name is gone.** `zenohd` is not on `PATH` and nothing installs it any
   more. `nros sdk-path zenohd`, `build/zenohd/zenohd` and
   `~/.nros/sdk/zenohd/…` name paths no recipe writes — 57 files still do.
2. **The flags are ignored.** `rmw_zenohd` takes **no command-line
   configuration**. It does not parse argv (`--help` starts a router), and reads
   `ZENOH_CONFIG_OVERRIDE` / `ZENOH_ROUTER_CONFIG_URI` instead. So a reader who
   obtains an `rmw_zenohd` and follows

   ```bash
   zenohd --listen tcp/127.0.0.1:7451 --no-multicast-scouting
   ```

   gets a router on the DEFAULT configuration — not the port they asked for, not
   with scouting disabled — and **no diagnostic**, because the flags are not
   rejected, they are unread. A wrong port is then a silent hang at
   `Executor::open`, which is the exact symptom the troubleshooting pages blame
   on other causes.

The correct form is the one `scripts/dev/zenohd.sh::nros_router_exec` already
encapsulates:

```bash
ZENOH_CONFIG_OVERRIDE='listen/endpoints=["tcp/127.0.0.1:7447"];scouting/multicast/enabled=false' \
    /opt/ros/$ROS_DISTRO/lib/rmw_zenoh_cpp/rmw_zenohd
```

## Scope, measured

`rg -l` over the tree, excluding `third-party/`, target dirs and lockfiles:

| class | files |
|---|---|
| flag invocations (`zenohd --…` / `-l`) | **95** |
| names a deleted path (`build/zenohd`, `sdk/zenohd`, `sdk-path zenohd`) | **57** |

Flag invocations by area: `docs` 35, `examples` 23, `packages` 16, `book` 11,
`scripts` 6, `just` 2, `tests` 1, `ci` 1.

The `docs/` share is mostly under `archived/`, where a historical record
describing what was true at the time is correct and must NOT be rewritten. The
live surfaces are `examples/`, `book/`, `packages/` (doc-comments at the head of
example sources), `scripts/`, `just/` and `tests/`.

**Two are executable, not prose:**

* `scripts/debug/debug-liveliness.sh:16` and
  `scripts/debug/compare-keyexprs.sh:34` actually run `zenohd --listen … &`.
  These now fail as command-not-found, which is at least loud.
* `ci/nano-ros-sdk/scripts/build-zenohd.sh` builds the retired package and is
  dead code.

The rest are `echo`/comment strings — instructions handed to a user at the
moment they are stuck, which is the worst time to be given a command that
silently does something else.

## Direction

1. **One helper, not 95 edits of the same shape.** `nros_router_exec` already
   exists and already encodes both halves (resolve the ROS binary, pass config
   by environment). Scripts and recipes should call it. This is the
   "add ONE shared helper rather than a second spelling" rule — the failure mode
   to avoid is fixing 95 sites into 95 slightly different `ZENOH_CONFIG_OVERRIDE`
   strings.
2. **Prose sites need a canonical snippet**, since a reader of `examples/**` has
   no access to a shell function. Pick one form, use it verbatim everywhere.
3. **Gate it.** A grep for `zenohd\s+(--|-l\b)` outside `docs/**/archived/**`
   should be a check — this class regenerates every time someone copies a
   neighbouring example's header comment, which is how it reached 95 files in
   the first place.
4. Delete `ci/nano-ros-sdk/scripts/build-zenohd.sh` with the rest of the retired
   package's build support.

## Not this issue

That a ROS-less host has no router to invoke by any spelling — that is
[issue 0653](0653-ros-less-host-has-no-zenoh-router.md). This issue is about the
instructions being wrong for the hosts that DO have one.
