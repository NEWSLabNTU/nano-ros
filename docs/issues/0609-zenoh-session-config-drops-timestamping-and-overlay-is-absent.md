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

| component | version |
| --- | --- |
| `ros-humble-rmw-zenoh-cpp` (apt) | 0.1.1 |
| `ros-humble-zenoh-cpp-vendor` (apt) | 0.1.1 |
| `zenohd` from the Eclipse apt repo | **1.4.0** |
| `zenohd` the tests actually spawn | **1.7.2** (`~/.nros/sdk/zenohd/1.7.2-nros2`) |

CLAUDE.md pins zenoh at 1.7.2 explicitly "(rmw_zenoh_cpp compat)", and the
overlay is the mechanism that keeps the C++ side on the matching build. Without
it, a 0.1.1 RMW vendored against the 1.4.0-era zenoh is speaking to a 1.7.2
router and our zenoh-pico pin. After cause 1 is fixed the session establishes and
**zero samples cross**, which is what a wire mismatch looks like.

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
3. **Then re-run the interop lane with the overlay built** and settle whether
   cause 2 is the version skew or something else.

Until (1) and (2) land, `just ci` cannot be green on any host that has ROS 2
humble but no `build/rmw_zenoh_ws` — which is every host that has not run the
overlay build.
