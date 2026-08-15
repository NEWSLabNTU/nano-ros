---
id: 609
title: The zenoh interop harness replaces rmw_zenoh_cpp's session config and drops `timestamping`, then falls back to a distro RMW that cannot talk to our router
status: open
type: bug
area: testing
related: [issue-0599, phase-311, rfc-0056]
---

## Symptom

Fifteen tests fail on a host with ROS 2 humble installed — every zenoh interop
cell, plus the workspace-feature lifecycle/QoS cells and both multi-node graph
tests:

```
interop::case_{1,2,3,4,5,9}_zenoh_*
workspace_features::case_{01_rust,06_c,11_cpp}_lifecycle
workspace_features::case_{08_c,13_cpp,17_mixed}_qos
qos_override_e2e, {rust,cpp}_multi_node_entry_per_node_graph_nodes
```

all with the same error from the ROS 2 side:

```
zenohc::publication_cache: Failed requirement for PublicationCache on
  0/rosout/rcl_interfaces::msg::dds_::Log_/TypeHashNotSupported:
  the 'timestamping' setting must be enabled in the Zenoh configuration
[ERROR] [rmw_zenoh_cpp]: Unable to make PublisherData.
error creating node: unable to create zenoh publisher cache
```

These are REAL failures, not `skip!`s: `name-real-failures.py` lists all fifteen.
They fail before the test's own assertions run, so the suite reports a delivery
problem that never got as far as delivery.

## Cause 1 — the generated session config replaces the shipped one

`rmw_zenoh_cpp` ships `DEFAULT_RMW_ZENOH_SESSION_CONFIG.json5`, which contains:

```json5
timestamping: {
  enabled: { router: true, peer: true, client: true },
  drop_future_timestamp: false,
},
```

`nros_tests::ros2::write_zenoh_session_config` (`ros2.rs:107`) writes a 20-line
config carrying only `mode`, `connect` and `scouting`, and
`ros2_env_setup_with_locator` points `ZENOH_SESSION_CONFIG_URI` at it. **That
variable REPLACES the shipped config; it does not merge with it.** So every
setting rmw_zenoh_cpp relies on and we did not restate is silently absent, and
`timestamping` is one of them.

`/rosout` is transient-local, which routes through zenoh-ext's
`PublicationCache`, which requires `timestamping`. Every ROS 2 node creates a
`/rosout` publisher, so this is not specific to interop — it is "no ROS 2 node
can start under our config".

**Verified by experiment.** Adding the block above to the generated config
changes the failure:

```
before:  [rmw_zenoh_cpp]: Unable to make PublisherData   (node never starts)
after:   nros → ROS 2 delivered nothing: 0 `data:` samples
```

The ROS 2 side now starts and subscribes. That is cause 1 confirmed — and it
uncovers cause 2, which the first error was hiding.

## Cause 2 — the wire-matched RMW overlay is absent, silently

`ros2::rmw_zenoh_overlay()` (`ros2.rs:24`) looks for
`build/rmw_zenoh_ws/install/setup.bash` — a pinned `rmw_zenoh_cpp` built, in its
own words, "wire-matched to our zenoh-pico pin". When it is missing the harness
appends nothing and falls through to the distro package. There is no message.

On this host it IS missing, so the tests run:

| component | zenoh version | in the failing run? |
| --- | --- | --- |
| `ros-humble-rmw-zenoh-cpp` -> vendored `libzenohc.so` | **1.2.0** | **yes** |
| `zenohd` from the Eclipse apt repo | 1.4.0 | no — never spawned |
| `zenohd` the tests spawn | **1.7.2** (`~/.nros/sdk/zenohd/1.7.2-nros2`) | **yes** |
| `zenoh-pico` pin (embedded side) | 1.7.2 | yes |

The RMW's zenoh version is 1.2.0, stated by the vendored library's own header —
not inferable from the apt package version, and not from the apt `zenohd`:

```c
/opt/ros/humble/opt/zenoh_cpp_vendor/include/zenoh_configure.h
#define ZENOH_C       "1.2.0"
#define ZENOH_C_MAJOR 1
#define ZENOH_C_MINOR 2
```

corroborated by `zenohcConfigVersion.cmake` and `zenohcxxConfigVersion.cmake`
(both `set(PACKAGE_VERSION "1.2.0")`). **The Debian versions 0.1.1 on
`rmw-zenoh-cpp` / `zenoh-cpp-vendor` are ROS wrapper-package versions and say
nothing about the zenoh inside** — an easy conflation, and the reason the number
here is read from the header rather than from `dpkg -l`.

The clincher that this library is the one failing: it was built from zenoh git
checkout `e4ea6f0`, which is the path in the error itself —
`…/git/checkouts/zenoh-cc237f2570fab813/e4ea6f0/zenoh-ext/src/publication_cache.rs:213`.
So the `timestamping` requirement is raised by zenoh-ext **1.2.0 inside
`libzenohc.so`**, not by our router.

CLAUDE.md pins zenoh at 1.7.2 explicitly "(rmw_zenoh_cpp compat)", and the
overlay is the mechanism that keeps the C++ side on the matching build. Without
it the live pairing is a **1.2.0 client against a 1.7.2 router** — five minor
versions, over a range in which zenoh 1.x changed behaviour (the `PublicationCache`
`timestamping` requirement being one such change). After cause 1 is fixed the
session establishes and **zero samples cross**, which is what that looks like.

Not proven to be the version skew specifically — only that delivery fails once
the node starts. Proving it needs a run with the overlay built, which this host
has not done.

## Why it reads as a nano-ros failure

Both halves fail QUIETLY in the direction that misleads:

* replacing a config silently drops what it does not restate;
* a missing overlay silently downgrades to a different RMW build.

Then a delivery assertion fires and names `rmw_zenoh` delivery. The test is
correct that delivery failed; it just cannot say the cause is two layers below
it. Same reporting shape as issue 0599 — a precondition missing at the point of
decision, surfacing later as a product-level failure.

## Direction

1. **Merge, do not replace.** Read the shipped
   `DEFAULT_RMW_ZENOH_SESSION_CONFIG.json5` and overlay our `connect`/`scouting`
   on it, so a future setting we do not know about is not dropped the same way.
   Restating `timestamping` alone fixes today's symptom and leaves the mechanism.
2. **Make the overlay fallback loud.** `rmw_zenoh_overlay()` returning `None`
   should be reported by the test that depends on it — ideally as
   `skip!` with a reason ("no wire-matched rmw_zenoh_cpp; distro 0.1.1 would be
   used"), so an unprovisioned host declares itself instead of producing fifteen
   delivery failures.
3. **Then re-run the interop lane with a matched RMW** and settle whether cause 2
   is the version skew or something else. Two ways to get one: build the pinned
   overlay (`build/rmw_zenoh_ws`), or upgrade the ROS packages to a release whose
   vendored zenoh matches the 1.7.2 pin. Either way the check is the same one
   line, and it is the header rather than `dpkg -l`, for the reason above:

   ```
   grep ZENOH_C /opt/ros/humble/opt/zenoh_cpp_vendor/include/zenoh_configure.h
   ```

   If that still reports a 1.2.x after an upgrade, the ROS distribution has not
   moved and the overlay is the only route.

Until (1) and (2) land, `just ci` cannot be green on any host that has ROS 2
humble but no `build/rmw_zenoh_ws` — which is every host that has not run the
overlay build.
