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

## Second pass 2026-08-19 — the canonical form was a literal, not a single source

The 2026-08-18 resolution replaced 92 unrunnable `zenohd --listen …` lines with
one canonical string. That fixed the reported defect and left a subtler one: the
string spells the router's install path.

```
ZENOH_CONFIG_OVERRIDE='listen/endpoints=["<loc>"];…' \
    /opt/ros/$ROS_DISTRO/lib/rmw_zenoh_cpp/rmw_zenohd
```

`/opt/ros/$ROS_DISTRO/...` is the **third** of the resolver's three steps
(issue 0653: `NROS_RMW_ZENOHD`, then `AMENT_PREFIX_PATH`, then that prefix). So
the canonical line is wrong on exactly the hosts step 2 was added for — a ROS
built from source, or a colcon overlay — and it was wrong in 92 places at once.
Copying one correct-looking literal 92 times is not a single source of truth;
it is 92 copies with a shorter changelog.

### What a single source needs, and what was missing

`nros_router_exec` already covered callers that can START the router. Nothing
covered callers that can only TELL somebody how, so nine of them hand-rolled the
same string — which is how the wrong path propagated.

* **`nros_router_hint [locator]`** (new, `scripts/dev/zenohd.sh`) prints the
  start line. It prints `ros2 run rmw_zenoh_cpp rmw_zenohd`: ROS's own
  documented invocation, needing only a sourced ROS, therefore correct wherever
  the router is installed. Verified to honour `ZENOH_CONFIG_OVERRIDE`.
* **`just zenohd [locator]`** (new, root) — the SSoT entry point for a human.
  Eight per-platform `just <plat> zenohd` recipes already delegated to
  `nros_router_exec` and differ only in locator; this is the same call without a
  platform, and the command documentation can name instead of pasting a line.

### Private copies deleted

* `tests/zephyr/run-c.sh` carried its **own three-step resolver**, written to
  "mirror" the Rust one, and had drifted: it still globs `/opt/ros/*` and takes
  the newest name — which issue 0653 REMOVED from both real resolvers because it
  returns a distro nobody chose — and it never learned `AMENT_PREFIX_PATH`. Now
  sources `nros_zenohd_bin`.
* `just doctor` CONSTRUCTED `/opt/ros/${ROS_DISTRO}/lib/...` and reported
  "not installed" for any ROS elsewhere. doctor telling a working host it is
  broken is the failure doctor exists to prevent. Now asks `nros_zenohd_bin` and
  prints the path it got.
* `scripts/debug/{capture-ros2-keyexpr,debug-keyexpr}.sh` and
  `scripts/qemu/setup-{network,qemu-network}.sh` composed the hint themselves;
  they call `nros_router_hint`.

73 further occurrences across 58 files rewritten from the path to the portable
command. The substitution is path→command with the surrounding quoting
untouched — deliberately unlike the first pass, whose failure was that it
restructured the whole line and broke five scripts twice.

### Gate extended rather than added

`check-zenohd-flag-invocations` now also rejects
`/opt/ros/<anything>/lib/rmw_zenoh_cpp/rmw_zenohd` outside the two resolvers,
their parity harness and the archived record. Mutation-checked: reintroducing
one literal fails it, reverting passes.

The exemption list is the honest statement of where this knowledge lives — the
shell resolver, the Rust resolver, the table that drives both, and RFC-0075.
Anywhere else asks one of them.
