# Phase 362 — The zenoh router comes from ROS, and the pairing is tested rather than pinned

**Status (2026-08-16). COMPLETE — W1-W5 all landed and verified.** Implements
[RFC-0075](../design/0075-zenoh-router-provenance-and-the-unstable-seam.md).
Opened out of [issue 0609](../issues/archived/0609-zenoh-session-config-drops-timestamping-and-overlay-is-absent.md),
whose two causes were fixed but which established that the pinning strategy
around them cannot work.

**Owns:** the vendored `zenohd` retirement, the interop lane's router
provenance, and the version-reporting that makes a convention break diagnosable.

**Related:** [RFC-0056](../design/0056-ros-edition-axis.md) (the ROS
edition axis this joins), [issue 0374](../issues/0374-zenohd-has-no-prebuilt-so-nros-setup-native-source-builds-it.md)
(the prebuilt promise the vendored router breaks),
[issue 0599](../issues/0599-zephyr-lane-reports-ok-when-it-skipped-everything.md)
(a lane that cannot run must say so — W3 depends on it).

## Why

Issue 0609 measured a pairing failure and a pairing fix inside a single ROS
distro: `ros-humble-rmw-zenoh-cpp` 0.1.1 → 0.1.9 moved its vendored zenoh
**1.2.0 → 1.8.0**, and interop went from zero samples to 10/10. Our own pin —
zenoh 1.7.2, held "for rmw_zenoh_cpp compat" — did not participate in either
outcome. It could not have: the thing that moved was `rmw_zenoh`'s own
conventions, which carry no version and no stability statement (RFC-0075).

Two facts decide this phase, and both are verified rather than assumed:

* `rmw_zenohd` ships with `rmw_zenoh_cpp` and links **the same `libzenohc.so`**
  the RMW does, so it cannot drift from it.
* **62 test files spawn our router; 2 spawn theirs.** In production a ROS 2
  deployment runs `rmw_zenohd`. We are testing a configuration nobody deploys.

The second is the argument that does not depend on any version number.

## Work items

### W1 — the interop lanes take the ROS router

Point every ROS-facing test at `ros2 run rmw_zenoh_cpp rmw_zenohd` instead of
`ZenohRouter`'s vendored binary. `ros_env.rs:625` already does this for the
edition lane, so the mechanism exists; this is extending it, not inventing it.

**Acceptance.** `interop_e2e` (10 cells), `workspace_features_e2e`,
`qos_override_e2e` and both multi-node graph binaries pass against the
ROS-shipped router, on a host with no `build/zenohd` at all — which is what
proves the vendored one is no longer load-bearing for these lanes.

### W2 — report the versions on failure

A zenoh convention break currently surfaces as `delivered nothing: 0 data:
samples`. Both numbers that would explain it are one file read away:

```
ZENOH_C  /opt/ros/<distro>/opt/zenoh_cpp_vendor/include/zenoh_configure.h
pico     packages/rmw/zenoh/zpico-sys/zenoh-pico/version.txt
```

**Read the header, never `dpkg -l`** — the package versions are ROS wrapper
versions (`0.1.9`) and say nothing about the zenoh inside (`1.8.0`). Reading them
is exactly what produced a wrong version claim in issue 0609's first filing, so
the helper should make the wrong source hard to reach.

**Acceptance.** A deliberately broken pairing prints both versions in the failure
message. Verified by planting a mismatch, not by reading the code.

### W3 — nano↔nano lanes stop needing our router

Two routes, and the phase should measure which applies per lane rather than
assume one:

* `zenoh-pico` supports `Z_CONFIG_MODE_PEER`, so some pairs may need no router;
* the rest use the ROS router when present and `skip!` when not — the honest
  verdict, per issue 0599, rather than silently testing a different router than
  production.

**Acceptance.** Every lane that used `ZenohRouter` either runs routerless, runs
on `rmw_zenohd`, or skips with a reason naming what is missing. **No lane
silently changes what it is testing.**

### W4 — retire the vendored `zenohd`

Delete the `third-party/zenoh/zenoh` submodule, the `zenohd` build recipe, and
the SDK-store entry. Blocked on W1 and W3.

Unaffected, and worth stating so the deletion is not over-scoped: the
`zenoh_archive_symbols` and header-parity fixtures consume
`nros-rmw-zenoh-staticlib`, not `zenohd`.

**Acceptance.** `just doctor` and `nros setup native` no longer build zenoh from
source; issue 0374's native+zenoh source-build is gone by construction rather
than by shipping a prebuilt.

### W5 — record the pairing as data

Add the observed-good pairing — `zenoh-pico 1.7.2 ↔ rmw_zenoh_cpp 0.1.9
(zenoh 1.8.0)` — to the book's support matrix beside the ROS edition axis. Data
a future failure gets diffed against, **not a constraint to enforce**: the RMW is
on the user's machine and we cannot pin it.

## Costs accepted

* **A ROS-less host cannot run the zenoh interop lanes.** Correct — those lanes
  test interoperation with ROS 2. Today they "run" against a router no user
  deploys, which is worse than skipping.
* **We stop controlling the router version.** Deliberate: control over a number
  we could not act on, traded for lockstep with the component it was meant to
  track.
* **`rmw_zenoh` can still break us in a patch release.** Unavoidable; W2 turns it
  from silent breakage into a named failure, which is the whole available
  improvement.

## Not measured

Whether the `workspace_features` / multi-node / QoS lanes have router
requirements the interop cells do not — they were only ever run against ours.
W1's acceptance will surface it.

Whether `zenoh-pico` should track the ROS-vendored zenoh at all. Under the zenoh
1.x wire guarantee it need not, and a firmware pin should move for its own
reasons — footprint, features, fixes — rather than to chase a host package. Out
of scope here; RFC-0075 says so explicitly.


## Outcome (2026-08-16)

All five work items landed. Verification notes, per item:

**W1 — the interop lanes take the ROS router.** Every `ZenohRouter` spawn runs
`rmw_zenoh_cpp/rmw_zenohd`. The leverage the plan hoped for was real: 134 call
sites across 66 files go through three constructors, so the provider moved in one
place and no test changed.

The mechanism was NOT the one the plan assumed. `rmw_zenohd` takes no
command-line configuration — it ignores argv (a `--help` starts a router) and
reads `ZENOH_CONFIG_OVERRIDE` / `ZENOH_ROUTER_CONFIG_URI`. So every `--listen` /
`--cfg` became an override entry. Verified against the installed binary rather
than inferred: `listen/endpoints=["tcp/127.0.0.1:17447"]` binds 17447 and not the
default 7447.

*Acceptance met:* `interop_e2e` (10 cells) + `workspace_features_e2e` +
`qos_override_e2e` = **31 tests, 31 passed, 0 skipped**, with
`build/zenohd/zenohd` moved off the host.

An earlier run of the same lanes showed 13 failures and they were NOT the router:
the fixtures were stale (a concurrent rebuild, then my own edits). The error said
so — "Test fixture is STALE — a source is newer than the built binary" — and
rebuilding the native lane turned all 13 green. Worth recording because a red
that arrives while you are changing a tree is evidence about the tree, not about
the change.

**W2 — report the versions on failure.** `zenoh_pairing_versions()` reads
`ZENOH_C` from `<ros>/opt/zenoh_cpp_vendor/include/zenoh_configure.h` and
zenoh-pico's `version.txt`, and is wired into the interop delivery assertion. It
offers no route to `dpkg -l`, as the plan asked.

*Measured here:* zenoh-c (ROS) **1.6.2** ↔ zenoh-pico **1.7.2** — NOT the
0.1.9 / 1.8.0 pairing this doc carried. The doc's numbers came from a different
install; W5 records what was measured.

**W3 — nano↔nano lanes.** Resolved as "runs on `rmw_zenohd`, or skips": `or_skip`
turns `RouterUnavailable` into `skip_class!(capability, …)`. The routerless
(`Z_CONFIG_MODE_PEER`) option was not needed — every lane the plan worried about
runs on the ROS router when it is present.

*Acceptance met, verified by removing the router* rather than by reading code:
with `NROS_RMW_ZENOHD` pointed at nothing, the ROS-peer cell reports
`[SKIPPED:capability] no rmw_zenoh_cpp/rmw_zenohd under /opt/ros … Pairing: …`.
No lane silently changed what it tests: the ones that moved from our router to
ROS's did so as the point of the phase, and it is written down.

**W4 — retire the vendored `zenohd`.** Submodule, `just/zenohd.just`,
`scripts/zenohd/build.sh`, the `[tool.zenohd]` SDK entry and `zenohd` from
`[rmw.zenoh] packages` all deleted. The eight `just <plat> zenohd` recipes still
start a router — the ROS one, via a shared `nros_router_exec`.

*Acceptance met:* `nros setup native --dry-run` now needs **2 packages**
(zenoh-pico, mbedtls), neither of them a router; issue 0374's native+zenoh
source build is gone by construction. `just doctor` no longer has a zenohd
section (its remaining failure is a missing corrosion install, unrelated).

Two things the deletion surfaced: `check-zenohd-spawn-sites` caught its own
obsolescence when the symbol it watched disappeared (and now watches
`ros_zenohd_path`), and `just doctor` died on the deleted module until the
zenohd entry was removed from the setup/doctor tiers.

**W5 — record the pairing as data.** In `book/src/design/rmw.md`, beside the RMW
comparison, with where to read each number and an explicit "not `dpkg -l`".

### The "Not measured" questions, answered

* *Do the workspace_features / QoS lanes have router requirements the interop
  cells do not?* **No.** All 31 pass on the ROS router with no per-lane
  special-casing.
* *Should `zenoh-pico` track the ROS-vendored zenoh?* Still out of scope, and
  this host is now a live example of why not: 1.7.2 firmware against 1.6.2 host,
  interoperating fine under the 1.x wire guarantee.

### Fallout fixed on the way

`1badb6f72` (phase-359 W10) moved `nros::init` / `ContextSource` behind an `env`
capability feature without updating `nros-tests`, whose `init_api.rs` exists to
exercise that API — the whole suite failed to COMPILE. Hidden until now because
`ci-matrix` was stopping at `check-feature-contract` (#643) before reaching the
tests.
