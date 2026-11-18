# Phase 574 — Perception data and large messages: feasibility, then the fixes it names

**Status (2026-11-17). OPEN: no item started. Owner: nros 241.** Opened by the
user's ruling of 2026-11-17: "Approve your study and have a phase doc. Be aware
of the scope. The nano-ros includes a board only if it meets a support
criteria. We may place board support package in the user project. My idea is
to check the feasibility and fix the nano-ros to enable the apps."

Each item is one PR, claimed with `just claim phase-574-wN` before it starts.

## Why

Two applications motivate this phase: a PX4 drone that uses onboard perception
for vision-based localization and moving-object rejection, and a precision
sprayer that fires nozzles from weed detections. In both, perception runs on a
Linux application processor with an accelerator, and a real-time context acts
on its results within a bound. That context is either a nano-ros node on the
SoC's real-time core or nano-ros inside PX4.

The capability both depend on is the transport of perception outputs through
nano-ros with bounded latency. Large payloads (images) matter only for some
deployments. Measured on main on 2026-11-17, that capability is not
established:

- The largest payload under test is 32 KB, zenoh, publish only
  (`packages/testing/nros-tests/tests/large_msg.rs`). Subscription at that
  size, and any size above it, is untested.
- zenoh-pico's embedded defaults are a 2048 B fragment and a 1024 B batch, and
  the build refuses a receive class larger than both (issue 1368's guard). A
  medium-class message therefore needs explicitly raised limits, and no
  configuration does that today.
- XRCE's practical limit was measured at about 2 KB embedded and 16 KB on
  POSIX (archived phase-40). Cyclone's is not measured.
- The `view` storage mode (RFC-0033), which exists for payloads such as
  `sensor_msgs/Image.data`, is implemented in codegen but has never been used
  at image size.
- Nothing places a receive pool or the executor arena in a memory region the
  board chooses. A real-time core with small tightly-coupled memory can hold
  a medium-class pool only in on-chip RAM or DDR.

## Scope

**In scope: nano-ros itself.** This means the RMW backends, the executor, codegen
and the sizing model, the tests and benchmarks, and the targets already in the
tree. The phase first measures, then fixes what blocks the applications.

**Out of scope:**

- **New boards.** nano-ros includes a board only when it meets the support
  criteria (RFC-0114 status words, RFC-0115 emulator-first policy). A board
  for a vision SoC (for example the i.MX 8M Plus Cortex-M7), and its RPMsg
  driver, belong in the user's project as an out-of-tree board package
  (RFC-0087). Where such a package needs a hook from nano-ros, for example a
  way to place a pool in a named memory section, providing the hook is in
  scope; the package itself is not.
- Perception models and pipelines, the simulation and emulation platform, and
  application code.

**Targets used:** only targets already in the tree:
- the native POSIX host;
- Zephyr `native_sim`;
- FreeRTOS on MPS2-AN385 (QEMU);
- Zephyr on FVP Cortex-R (AEMv8-R);
- the Nucleo-F767ZI under Renode, as the Cortex-M7 proxy for an M7-class real-time core.

## Message classes

The sizes below are computed from the standard message definitions, not
measured. W1 confirms them against real node outputs.

| Class | Examples | Size per message | Rate | Path |
|---|---|---|---|---|
| Small | nozzle command, setpoint, health | under 1 KB | 100–1000 Hz | real-time core ↔ actuators and PX4 |
| Medium | `nav_msgs/Odometry` (about 0.7 KB); `vision_msgs/Detection2DArray` (about 0.4 KB per detection, so 4–40 KB); a JPEG thumbnail (30–150 KB) | 1–150 KB | 1–200 Hz | perception → real-time core |
| Large | `sensor_msgs/Image` VGA mono8 to RGB (0.3–0.9 MB); `PointCloud2` 640×480 (about 4.9 MB) | 0.3–6 MB | 10–30 Hz | camera node or Linux ↔ Linux |

The medium class decides feasibility for both applications. The large class
decides only whether an MCU-class camera node is viable later.

## Work items

### W1 — Requirements, confirmed

- **Owner:** nros 241.
- **Status:** OPEN
- Confirm the message-class table from the outputs of one open-source
  visual-inertial odometry node and one detector, with their real field
  counts and rates. Record which message types and sizes each application
  needs at the real-time core.
- **Acceptance:** a requirements table in this document, giving each class's
  size, rate and latency budget, with its source.

### W2 — Capability matrix

- **Owner:** unassigned.
- **Status:** OPEN
- For payload sizes of 1, 16, 64 and 256 KB, 1 MB and 4 MB, run publish and
  subscribe on each in-tree RMW (zenoh, XRCE, Cyclone) and each target listed
  under Scope. Cover the `inline` and `view` storage modes where they apply.
  This extends the existing large-message test and the `large-msg-xrce` and
  `large-msg-baremetal` benchmarks; it does not add a test file per cell.
- **Acceptance:**
  - a matrix of pass or fail per cell, with the first failure mode and the
    configuration it needed (fragment, batch and receive-class sizes);
  - every passing cell asserts the payload's content, not only its arrival;
  - the cells that must keep passing join the test lanes, so a regression is
    caught.

### W3 — Memory cost and placement

- **Owner:** unassigned.
- **Status:** OPEN
- Measure the RAM each message class costs per configuration in W2 with
  `just mem-report` and the sizing descriptor (RFC-0100). On the Cortex-M7
  proxy, determine whether a medium-class receive pool fits the
  tightly-coupled memory, and if not, whether it can live in other on-chip
  RAM.
- If a board must choose where the pools and the arena live, specify the hook
  nano-ros provides for an out-of-tree board package to make that choice, for
  example a named linker section per pool. Record it as an RFC amendment or a
  new RFC before it is built.
- **Acceptance:** bytes per message class per target; a stated answer on
  whether the medium class fits an M7-class memory map; and, if a hook is
  needed, its accepted design.

### W4 — Timing under load

- **Owner:** unassigned.
- **Status:** OPEN
- Deliver a 20 KB detection array at 30 Hz to a nano-ros node while a 1 kHz
  control task runs in a higher tier (RFC-0047 tiers, RFC-0113 wake). Measure
  delivery latency (p50, p99, maximum) and the control task's period jitter,
  idle and under load.
- On the host, the timing is indicative. On emulators, only functional
  behaviour counts, because emulator timing is not cycle-accurate.
- **Acceptance:** latency distributions and the control-task interference,
  with the conditions under which they were measured.

### W5 — Small-MTU link

- **Owner:** unassigned.
- **Status:** OPEN
- An inter-core RPMsg link carries at most 512 B per buffer. Constrain a
  serial or TCP link on the host to a 496 B MTU, then repeat W2's medium
  sizes and W4's measurement over it. This predicts the behaviour of an RPMsg
  path before any board package provides one.
- **Acceptance:** throughput and latency against MTU for the medium class, and
  whether zenoh-pico's fragmentation carries it correctly at that MTU.

### W6 — Feasibility verdict

- **Owner:** nros 241.
- **Status:** OPEN
- Apply the criteria under Acceptance below to W1–W5. Record the verdict, and
  list each nano-ros defect or limit that blocks the applications as an issue
  homed here, with its measurement.
- **Acceptance:** a verdict section in this document, and one issue per
  blocking defect, each linked from the table under Issues homed here.

### W7 — Fixes named by W6

- **Owner:** unassigned.
- **Status:** OPEN
- One PR per issue filed in W6, in the order W6 ranks them. Each fix is
  within the scope above; a fix that needs a board-side change ships the
  nano-ros hook and documents what the board package supplies.
- **Acceptance:** every blocking issue from W6 is resolved, and the W2, W4
  and W5 measurements that failed now pass.

## Acceptance for the phase

The applications are feasible on nano-ros when all of these hold:

- **Medium class, both directions.** Messages up to 64 KB pass, publish and
  subscribe, on zenoh on at least one embedded target in the tree.
- **Memory.** The medium class fits an M7-class memory map, with the hook from
  W3 if one is needed.
- **Latency.** At 30 Hz, the p99 delivery latency on the host stays below one
  frame period (33 ms).
- **No interference.** The control task's period shows no measurable change
  under that load.
- **Small MTU.** The medium class passes over a 496 B MTU link.

If a criterion fails, W6 names the limiting component and W7 fixes it. The
phase closes when every criterion holds, or when the user rules that a failed
one is out of reach and records why.

## Related

- RFC-0010 (zero-copy raw API), RFC-0033 (per-field capacity and storage
  modes), RFC-0038 (in-place receive and slot pools), RFC-0100 (sizing model),
  RFC-0113 (unified wait), RFC-0047 (callback groups and sched tasks).
- RFC-0087 (out-of-tree packages), RFC-0114 and RFC-0115 (board support
  criteria), which set the scope boundary above.

## Issues homed here

| issue | why it belongs here |
| --- | --- |
| [2696](../issues/2696-executor-wake-from-interrupt-not-wired.md) | A device job's completion interrupt cannot wake the executor safely, which the accelerator and camera paths need |
