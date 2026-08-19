---
rfc: 0075
title: "Zenoh router provenance: take the router from ROS, pin only what compiles into firmware"
status: Draft
since: 2026-08
last-reviewed: 2026-08
implements-tracked-by: [phase-362]
supersedes: []
superseded-by: null
---

# RFC-0075 — Zenoh router provenance: take the router from ROS, pin only what compiles into firmware

## Summary

nano-ros currently builds and ships its own `zenohd`, pinned to zenoh 1.7.2 via
the `third-party/zenoh/zenoh` submodule, and pairs it with a vendored
`zenoh-pico` pinned to the same version. The pin exists "for rmw_zenoh_cpp
compat", which is a version-matching strategy against a moving target: the zenoh
inside `ros-humble-rmw-zenoh-cpp` changes *within one ROS distro*, and did — 0.1.1
to 0.1.9 moved it from **1.2.0 to 1.8.0** (issue 0609).

This RFC proposes:

1. **Take the router from the ROS installation** (`rmw_zenohd`, shipped by
   `rmw_zenoh_cpp`) for every path that talks to ROS 2, and retire the vendored
   `zenohd` build. This is not a better pin — it is a **lockstep**, because that
   router links the same `libzenohc.so` the RMW does, by construction.
2. **Keep pinning `zenoh-pico`**, because it compiles into firmware and cannot
   come from a package manager.
3. **Replace version pinning with a conformance gate** on the pairing, because
   the surface we actually bind to carries no stability guarantee and therefore
   no pin can protect it.

## Motivation

### Two axes, only one of which we can choose

| axis | where it runs | can it come from ROS? |
| --- | --- | --- |
| the **router** | a host process | **yes** |
| `zenoh-pico` | linked into firmware | **no** |

Treating these as one "zenoh version" is what produced the current design. They
have different constraints and should be decided separately.

### What is guaranteed, and what is not

**The zenoh wire protocol is stable within 1.x.** Eclipse committed to backward
compatibility from 1.0.0 onward, and the 1.1.0 release notes restate it for
subsequent fixes. Our `zenoh-pico` speaks `Z_PROTO_VERSION 0x09`
(`zenoh-pico/include/zenoh-pico/config.h:282`); that constant is what the
guarantee covers.

**`rmw_zenoh`'s ROS-level conventions carry no guarantee at all.** Its
`docs/design.md` specifies the keyexpr format, liveliness-token format,
attachment encoding and the QoS→keyexpr packing in full detail, and attaches **no
stability statement, no version field, and no "subject to change" note**. The
package ships as `0.1.x` — pre-1.0 — and its README documents no version-matching
requirement between the RMW and a router.

So the layer nano-ros binds to is precisely the undocumented one. That asymmetry
is the whole of this RFC's reasoning:

> We pin against the layer that promises stability, and test against the layer
> that does not.

### The evidence

Issue 0609 measured, on one host, one afternoon:

| pairing | result |
| --- | --- |
| rmw_zenoh 0.1.1 (zenoh 1.2.0) ↔ our zenohd 1.7.2 | session established, **zero samples** |
| rmw_zenoh 0.1.9 (zenoh 1.8.0) ↔ our zenohd 1.7.2 | **delivers** |

Both pairings are inside zenoh 1.x, where the wire is guaranteed compatible. So
the failure was **almost certainly not the wire** — it is far more likely to have
been rmw_zenoh's own conventions moving between 0.1.1 and 0.1.9. The measurement
does not separate the two, and this RFC does not claim it does; what it
establishes is that *a version pin on zenoh did not protect us*, and could not
have, because the thing that moved was not versioned.

### What the vendored router costs

* a submodule (`third-party/zenoh/zenoh`) and a from-source `zenohd` build,
  reachable only after `nros setup` — issue 0374 is the book promising prebuilts
  it does not ship;
* a standing obligation to re-pin whenever ROS moves, with no signal that it has;
* a test suite that validates against **a router no user runs**. In production a
  ROS 2 deployment runs `rmw_zenohd`. Our 62 test files spawn ours; two spawn
  theirs.

That last point is the strongest argument and is independent of every version
number: **we are testing a configuration nobody deploys.**

## Design

### D1 — The router comes from ROS wherever ROS is involved

`ros2 run rmw_zenoh_cpp rmw_zenohd`, resolved through the same ament install the
RMW comes from. Verified property, not an assumption:

```
/opt/ros/humble/lib/rmw_zenoh_cpp/rmw_zenohd
  → libzenohc.so  (/opt/ros/humble/opt/zenoh_cpp_vendor/lib)   # the RMW's own
```

Router and RMW cannot drift, because an apt upgrade moves both or neither. No pin
is required and none is possible — which is the point.

### D2 — `zenoh-pico` stays vendored and pinned

It links into firmware; there is no package-manager story for it, and the zenoh
1.x wire guarantee is what makes a fixed pin safe against a moving router.

### D3 — The vendored `zenohd` build is retired

The `third-party/zenoh/zenoh` submodule and the `zenohd` build recipe go. Two
consumers need attention rather than deletion:

* **nano↔nano lanes**, which need *a* router but not a ROS one. `zenoh-pico`
  supports `Z_CONFIG_MODE_PEER`, so some may need no router at all; the rest can
  use the ROS router when present and `skip!` when not — honestly, since a lane
  that cannot run should say so (issue 0599).
* **`zenoh_archive_symbols` / header-parity fixtures**, which inspect a built
  archive rather than run a router. These consume `nros-rmw-zenoh-staticlib`, not
  `zenohd`, and are unaffected.

### D4 — Conformance replaces pinning

Because the unstable surface is undocumented, only a test can defend it. The
existing `interop_e2e` cells *are* that test; they need two properties they lack:

1. **Run against `rmw_zenohd`**, so what is validated is what is deployed.
2. **Report both versions on failure** — `ZENOH_C` from
   `/opt/ros/<distro>/opt/zenoh_cpp_vendor/include/zenoh_configure.h` and our
   `zenoh-pico/version.txt`. Today a convention change surfaces as "0 samples
   delivered", which is a full day of bisection away from its cause.

**Read the header, never the package version.** `dpkg -l` reports
`ros-humble-rmw-zenoh-cpp 0.1.9` and `ros-humble-zenoh-cpp-vendor 0.1.9`; those
are ROS wrapper-package versions and say nothing about the zenoh inside. Reading
them is what produced a wrong version claim in issue 0609's first filing.

### D5 — The validated pairing is data, not a constraint

Record `zenoh-pico 1.7.2 ↔ rmw_zenoh_cpp 0.1.9 (zenoh 1.8.0)` as an observed-good
pairing in the book's support matrix, beside the ROS edition axis (RFC-0056). It
is what a future failure gets diffed against — not a version to enforce.

## Consequences

**Accepted.**

* **A ROS-less host cannot run the zenoh interop lanes.** Correct: those lanes
  test interoperation with ROS 2. They should `skip!` with a reason rather than
  run against a router no user deploys.

  *Amended 2026-08-18 (issue 0653): this consequence was written too narrowly.*
  zenoh-pico connects in CLIENT mode, so a router is needed by **any two-process
  zenoh example**, interop or not — a talker and a listener are two processes.
  The casualty is therefore not the interop lanes but the whole `--rmw zenoh`
  path on a ROS-less host, including the getting-started one, since zenoh is the
  default RMW. Confirmed and accepted rather than reversed: nano-ros does not
  ship a router. `--rmw cyclonedds` is the ROS-less route (in-process, no
  daemon), the book's getting-started page says so, and `NROS_RMW_ZENOHD` remains
  the escape hatch for a router obtained some other way.

  What *was* wrong is that "has ROS" was implemented as "has `/opt/ros`". Both
  resolvers now read the SOURCED environment — `AMENT_PREFIX_PATH` first, then
  the `/opt/ros` fallbacks — so a ROS built from source or installed under a
  colcon overlay resolves the router the moment its `setup.bash` is sourced.
  Note `rmw_zenohd` is NOT put on `PATH` by that sourcing: it installs into
  `lib/rmw_zenoh_cpp/`, and ROS's own route to it is `ros2 run rmw_zenoh_cpp
  rmw_zenohd`. `AMENT_PREFIX_PATH` is what makes it findable, which is why the
  fallback that skipped it was the defect.

  **Nothing is searched that the user did not name**, and this RFC is the
  reason. Its argument is not "get a router" but "get the router PAIRED with the
  `rmw_zenoh_cpp` in use" — the pairing being the thing a version number could
  not express. Two searches were tried and both removed:

  * **`PATH`.** Where an unpaired router accumulates. The host this was written
    on carried *two*: `/usr/bin/zenohd` **v1.4.0** from a system install, and a
    retired `zenohd` **1.7.2** in `~/.nros/sdk` that nano-ros itself was still
    putting on `PATH`, against ROS's zenoh-c **1.8.0**.
  * **`/opt/ros/*`, newest name last.** The same mistake with better disguise:
    on a host with humble and jazzy both installed it returns jazzy by
    collation, whatever the user sourced. Worse than the `PATH` case, because
    both candidates are genuine ROS routers, so nothing about the answer looks
    wrong and a lane can run green against the distro nobody was testing.

  What remains is `NROS_RMW_ZENOHD`, `AMENT_PREFIX_PATH` and `$ROS_DISTRO` —
  each a statement by the user. Both resolvers additionally WARN when what
  `NROS_RMW_ZENOHD` names is not `<prefix>/lib/rmw_zenoh_cpp/rmw_zenohd` beside
  a `zenoh_cpp_vendor` header: the override is a legitimate act, but it must not
  be a silent one.

  Retiring the router from the SDK index (phase-362) turned out not to retire it
  from hosts: the store accumulates by design, and `scripts/sdk-path-tools.txt`
  still wired the entry onto `PATH`. `just doctor` now reports a retired store
  entry that is still installed.
* **We no longer control the router version.** That is the trade this RFC makes
  deliberately: control over a number we could not use, exchanged for lockstep
  with the component that number was supposed to track.
* **`rmw_zenoh` can still break us in a patch release.** Pinning never prevented
  this — the RMW is on the user's machine. D4 turns it from a silent breakage
  into a named test failure, which is the whole available improvement.

**Not addressed here.** Whether `zenoh-pico` should track the ROS-vendored zenoh
version at all. Under the 1.x wire guarantee it need not, and a firmware pin
should move for reasons of its own (features, footprint, fixes) rather than to
chase a host package.

## Alternatives considered

**Keep the vendored router, pin harder.** Rejected: the 1.2.0→1.8.0 move happened
inside one ROS distro with no signal, so there is no version to pin *to*. It also
preserves the deeper flaw — validating a router nobody runs.

**Vendor `rmw_zenoh_cpp` too, and pin the whole ROS side.** This was the
`build/rmw_zenoh_ws` overlay, built from a `third-party/zenoh/rmw_zenoh`
submodule. Rejected as the default because it inverts the goal: users run the
distro's RMW, so pinning ours makes the tested configuration *less* like
production, not more.

*Amended 2026-08-19: the overlay and its submodule are DELETED, not kept as an
opt-in.* This RFC originally kept them for "reproducing a specific pairing", and
in the time since, nothing ever did. Measured before removing: the submodule was
never initialised in any checkout; its only references were the recipe that
initialised it on demand; the `ros-editions` image installs the apt package and
left the source pin as a `WITH_ZENOH_PIN` layer that was never written; and
every harness call site began from the distro install and layered the overlay
only if it happened to exist. The pairing rationale had also been refuted
independently by issue 0291 — zenoh's wire is proto-`0x09`-stable across 1.x, so
zpico 1.7.2 interoperates with a far newer distro RMW, and that investigation's
real finding was the keyexpr type-hash. An opt-in nobody opts into is a
maintenance surface with a story attached, and the story was wrong.

**Require the overlay whenever ROS is present.** Rejected: after the 0.1.9
upgrade the distro RMW interoperates, so requiring an hour-long build to run the
suite would be cost without benefit. That reasoning is what eventually removed
the overlay outright (above) — if it is never required and never chosen, it is
not a fallback, it is dead weight.

→ issue 0609 (both causes and the measurements), RFC-0056 (ROS edition axis),
issue 0374 (the prebuilt-toolchain promise), issue 0599 (a lane that cannot run
must say so).
