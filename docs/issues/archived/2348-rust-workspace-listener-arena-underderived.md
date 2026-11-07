---
id: 2348
title: "A scaffolded Rust workspace's listener dies at boot with `arena exhausted at arena::SubBufferedRawEntry: 9124 B short`: the descriptor prices its subscription as `typed_bound`, and the runtime registers a buffered raw entry"
status: resolved
type: bug
area: core, sizing, cli
severity: high
purpose: [education, industrial]
related: [phase-523, issue-2346, issue-1648, issue-1190, phase-457]
found: 2026-11-07
resolved_in: "probe-checkout-ros2 branch: the Rust probe states the declared shape's buffered row beside in_place; the observation outranks the backend default"
---

**Home: [phase-523](../../roadmap/phase-523-education-labs-adoption-probes.md)**. Found fixing
issue 2346: once the checkout probe had ROS 2, it got past configure, the C++
quick start passed, and the next step, the Rust quick start, failed here.
Before that, the `AMENT_PREFIX_PATH` refusal stopped the probe ahead of this
step, so it did not show.

## Measured

On `main` at `68c638b3fd`, on a Linux host with Humble sourced, using the
checkout's `nros`:

```text
nros new probe_quickstart_rs --workspace --lang rust
cd probe_quickstart_rs && nros configure && nros build      # both exit 0
./build/posix-cyclonedds/linux_entry/target/debug/linux_entry
[ERROR] nros: arena exhausted at arena::SubBufferedRawEntry: 9124 B short, 192/3356 used.
        Raise NROS_PUBSUB_QOS_DEPTH (budgeted 10) if a sub is deeper, else
        NROS_EXECUTOR_ARENA_SIZE or NROS_EXECUTOR_ACTION_CLIENTS (Zephyr: CONFIG_*).
[ERROR] nros: node declaration failed — NodeError::BufferTooSmall
nros: application error: NodeRegister("listener_pkg")
```

The probe container (`ros:jazzy`, `PROBE_TRACK=quickstart`) shows the same
lines. The scaffold writes no `system.contract.yaml`, so no contract is
involved.

The image's sizing descriptor
(`build/posix-cyclonedds/linux_entry/nros/sizing/linux.toml`, `basis = "probe"`)
states the listener's one subscription as:

```text
kind = "subscription"  type = "std_msgs/msg/Int32"  registration_path = "typed_bound"
wire_bound_bytes = 12
[endpoint.refused] storage_bytes = "no `depth` was declared for this endpoint …"
```

The runtime registers it as `SubBufferedRawEntry`, the closure-buffer
(`RX_BUF`) row. The arena is derived for a 12-byte bound, and the registration
needs about 9 KB more.

## Lead (not a diagnosis)

The listener calls `node.create_subscription_for_callback_name::<Int32>(…)`.
The metadata probe records that registration's buffered row as `typed_bound`
(issue 1648's observation, `node_metadata.rs` `BufferedRow`). The runtime path
from that call reaches a raw buffered entry. Either the observation is wrong
for this entry point, or the runtime takes a different funnel than the probe
saw. Find where the two diverge before changing either side. Issue 1190 is the
same symptom from a different cause: a QoS history priced at 3 instead of 11.

## Next

Fix where the registration path is decided, so the probe observes what the
runtime does. Acceptance: the checkout probe's Rust quick-start step prints
`Publishing: 1`.

## Resolution (2026-11-07)

**Cause.** Two halves of one observation were split.

- A Rust probe declares without registering (issue 1522), so
  `MetadataRecorder::create_entity` states the subscription's
  `in_place_capable` from `DeclaredSubscriptionShape`. It never stated
  `buffered_row`, although `node_metadata.rs` says the two are "present
  together or not at all". The sidecar carried `"in_place": true` and no
  `buffered`.
- The composer (`sizing_descriptor.rs`) answered a descriptor-carrying backend
  (Cyclone) with `TypedBound` before it read any observation.

So the listener was priced at its type's 12-byte bound. The declarative
runtime registers through the type-erased `BufferedRaw` shape, which claims
`RX_BUF` on every backend, and Cyclone does not dispatch in place, so the image
needed a buffered region it had not been given.

**Fix.**
- `DeclaredSubscriptionShape::claims_closure_buffer()`: both declared shapes
  register with `slot_bytes: RX_BUF`.
- The recorder states `buffered_row` from it, beside `in_place_capable`.
- The composer reads the observation before the backend default.
- `check-declared-subscription-shape` now requires the recorder to state both,
  and has a new mutation that removes the buffered half.

**Verified.** The same scaffold, rebuilt with the fixed CLI: the sidecar says
`"buffered": "unbounded"`, the descriptor's subscription row says
`registration_path = "unbounded"`, and `linux_entry` prints `Publishing: 1` and
`I heard: 1`.
