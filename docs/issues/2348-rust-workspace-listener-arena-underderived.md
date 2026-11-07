---
id: 2348
title: "A scaffolded Rust workspace's listener dies at boot with `arena exhausted at arena::SubBufferedRawEntry: 9124 B short`: the descriptor prices its subscription as `typed_bound`, and the runtime registers a buffered raw entry"
status: open
type: bug
area: core, sizing, cli
severity: high
purpose: [education, industrial]
related: [phase-523, issue-2346, issue-1648, issue-1190, phase-457]
found: 2026-11-07
---

**Home: [phase-523](../roadmap/phase-523-education-labs-adoption-probes.md)**. Found fixing
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
