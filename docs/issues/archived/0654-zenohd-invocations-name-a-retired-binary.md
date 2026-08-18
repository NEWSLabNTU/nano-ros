---
id: 654
title: "Every `ZENOH_CONFIG_OVERRIDE='listen/endpoints=["…`"];scouting/multicast/enabled=false' /opt/ros/$ROS_DISTRO/lib/rmw_zenoh_cpp/rmw_zenohd in the tree names a binary that no longer exists and passes flags the replacement ignores — ~95 files, two of them executable"
status: resolved
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
   ZENOH_CONFIG_OVERRIDE='listen/endpoints=["tcp/127.0.0.1:7451"];scouting/multicast/enabled=false' /opt/ros/$ROS_DISTRO/lib/rmw_zenoh_cpp/rmw_zenohd
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
  `scripts/debug/compare-keyexprs.sh:34` actually run `ZENOH_CONFIG_OVERRIDE='listen/endpoints=["…"];scouting/multicast/enabled=false' /opt/ros/$ROS_DISTRO/lib/rmw_zenoh_cpp/rmw_zenohd &`.
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


## RESOLVED 2026-08-18

All four directions landed.

**1. Executables first, because those fail rather than mislead.**
`scripts/debug/{debug-liveliness,compare-keyexprs}.sh` now source
`scripts/dev/zenohd.sh` and call `nros_router_exec` (it `exec`s, so it runs in a
subshell). Both also tracked the router with `pgrep -x zenohd` / `pkill -x
zenohd`, which cannot match `rmw_zenohd`; they now kill by the PID they started.
`pkill -f <pattern>` is gone from both — the pattern matches the shell running it
as readily as the target, a self-match that killed the caller six separate times
while this issue was being worked.

**A live breakage the survey had not named:** `build-all.mk:75` still ran
`just build-zenohd`, a recipe phase-362 deleted — `error: justfile does not
contain recipe 'build-zenohd'`. That stage is removed, and
`ci/nano-ros-sdk/scripts/build-zenohd.sh` deleted with it.

**2. One canonical prose form**, applied to 81 invocations across 62 files:

```bash
ZENOH_CONFIG_OVERRIDE='listen/endpoints=["<loc>"];scouting/multicast/enabled=false' \
    /opt/ros/$ROS_DISTRO/lib/rmw_zenoh_cpp/rmw_zenohd
```

`book/src/user-guide/serial-transport.md` was the one site needing a different
key — it dials rather than binds, so `connect/endpoints`. A blanket rewrite would
have silently given it the wrong one.

**3. Gated** by `check-zenohd-flag-invocations` (`check-fast`), mutation-checked
in both directions: reintroducing a `zenohd --listen` line fails it, reverting
passes. `docs/**/archived/**` is exempt — 29 files whose historical record is
correct as written.

### Note on the rewrite, since it went wrong twice

The first pass broke five shell scripts: the replacement's inner `"` terminated
the enclosing `echo "…"`. The second pass "fixed" it by escaping, but the locator
character class still swallowed the closing quote, so the scripts stayed broken
in a new way — and the edit that was supposed to fix the class had silently not
applied at all (a `str.replace` with no assertion, which is its own recurring
lesson). `bash -n` over every modified script is what caught both rounds; a
grep-based sweep of this size needs a syntax check as its acceptance, not a diff
skim.
