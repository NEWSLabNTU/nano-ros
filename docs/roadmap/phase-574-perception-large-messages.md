# Phase 574 — Perception data and large messages: feasibility, then the fixes it names

**Status (2026-11-26). OPEN: W2 and W3 started. Owner: nros 241 (W2 and W3: nros 175).** Opened by the
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
- **Status:** DONE (2026-11-26). The table is below.
- Confirm the message-class table from the outputs of one open-source
  visual-inertial odometry node and one detector, with their real field
  counts and rates. Record which message types and sizes each application
  needs at the real-time core.
- **Acceptance:** a requirements table in this document, giving each class's
  size, rate and latency budget, with its source.

#### W1 result: requirements (2026-11-26)

**The two nodes.**

- **VIO: OpenVINS**, the ROS 2 node `run_subscribe_msckf`
  (`ov_msckf/src/ros/ROS2Visualizer.cpp`, launched by
  `ov_msckf/launch/subscribe.launch.py` in namespace `ov_msckf`), with its
  shipped EuRoC configuration.
- **Detector: `mgonzs13/yolo_ros`**, release 4.7.1 (`yolo_node.py`,
  `yolo_bringup/launch/yolo.launch.py`). Its `main` has since become a C++
  node with per-camera topics. Its message, `yolo_msgs/DetectionArray`, is
  unchanged in shape, so the sizes below hold for both versions.
  The generic `vision_msgs/Detection2DArray` is sized as well, because a
  detector that publishes the standard type pays its per-detection cost.

**How the sizes were taken.** The `nav_msgs`, `sensor_msgs` and
`vision_msgs` sizes are the serialized length (CDR, including the 4-byte
encapsulation header) from `rclpy.serialization.serialize_message` on ROS 2
Humble, with each node's own frame ids. `yolo_msgs` is not packaged for
Humble here. Its size is computed from the 4.7.1 `.msg` definitions with the
same alignment rules, filling the fields the node fills for a plain
detection model: `class_id`, `class_name` (`"person"`), `score` and `bbox`.
`id`, `bbox3d`, `mask` and the keypoints are left empty.

| Message | Reaches the real-time core | Fields that grow | Size | Rate | Source of the rate |
| --- | --- | --- | --- | --- | --- |
| `nav_msgs/Odometry` on `/ov_msckf/odomimu` | yes (drone: PX4 external vision) | none; both 6x6 covariances filled | 716 B | about 200 Hz | one per IMU sample (`callback_inertial` calls `visualize_odometry`, with no rate limit); EuRoC IMU `update_rate: 200.0` |
| `geometry_msgs/PoseWithCovarianceStamped` on `poseimu` | no (Linux side) | none | under 0.4 KB | about 20 Hz | once per processed camera frame; EuRoC `track_frequency: 21.0` |
| `sensor_msgs/PointCloud2` on `points_msckf` and `points_slam` | no (visualization) | 12 B per feature (`x`, `y`, `z` float32) | 113 B + 12 B per point: 713 B at `max_slam: 50` | about 20 Hz | once per camera frame |
| `yolo_msgs/DetectionArray` on `/yolo/detections` | yes (drone: moving-object rejection; sprayer: nozzle decision) | about 192 B per detection | 48 B + about 192 B each: 1,965 B at 10, 19,245 B at 100, 57,645 B at `max_det: 300` | one per input image, no throttle; 19 FPS on the original Jetson Nano | `image_cb` predicts and publishes per frame; Qengineering's YOLOv8n TensorRT FP16 table, inference only |
| `vision_msgs/Detection2DArray` (same detections, standard type) | yes, where the detector publishes the standard type | 440 B per detection, most of it the hypothesis pose's 6x6 covariance | 25 B + 440 B each: 4,425 B at 10, 44,025 B at 100, 132,025 B at 300 | as the detector | as above |
| `sensor_msgs/Image`, EuRoC camera, 752x480 mono8 | no (VIO input, Linux) | `data` | 361,012 B | 20 Hz per camera, stereo | EuRoC camera chain |
| `sensor_msgs/Image`, detector input, 640x480 rgb8 | no (detector input, Linux) | `data` | 921,652 B | camera rate | `yolo.launch.py` default 640x480 |

**What the real-time core receives, per application.**

| Application | Message | Size per message | Rate | Latency budget | Source of the budget |
| --- | --- | --- | --- | --- | --- |
| Drone, localization | `nav_msgs/Odometry` | 716 B | 30–50 Hz into PX4 (OpenVINS publishes at about 200 Hz, so the bridge decimates) | the estimate's age must be stable and is compensated up to `EKF2_EV_DELAY`'s maximum of 300 ms; this phase sets transport at below one 30 Hz period (33 ms) | PX4 "External Position Estimation": "streamed at between 30Hz (if containing covariances) and 50 Hz", and below that "EKF2 will not fuse"; `EKF2_EV_DELAY` in `params_external_vision.yaml`, 0–300 ms |
| Drone, moving-object rejection | detection array | 2–20 KB (10–100 detections, `yolo_msgs`) | camera rate, 19–30 Hz | one frame period, 33 ms at 30 Hz, so the result applies to the frame it came from | this phase's latency criterion; no external figure found |
| Sprayer, nozzle decision | detection array | 2–58 KB with `yolo_msgs`, 4–132 KB with `vision_msgs` (10 to 300 detections) | camera rate | 180 ms end to end, from capture to spray, at about 20 km/h with a 1 m field of view; transport takes its 33 ms share, leaving the rest to capture, inference (53 ms at 19 FPS on the Nano) and the valve | Bilberry patent US11071991B2: "the latency time (tl) has to be less than 180 ms", travel at "between 7 and 25 km/h" |

**What changes in the class table above.**

- The two predicted medium sizes hold: `nav_msgs/Odometry` is 716 B, and a
  `vision_msgs` detection is 440 B.
- The medium class's upper end is set by the detection count, not by a
  thumbnail. A `vision_msgs` array at the detector's `max_det: 300` is
  132 KB. No measured application here sends a JPEG to the real-time core.
- Images stay on the Linux side in both applications. The large class is
  needed only for a future MCU-class camera node, as the class table says.
- The small class (setpoints, nozzle commands, health) is not confirmed by
  these two nodes. Its sizes stay computed.

**Limits the table runs into, from W2 so far.**

- **XRCE:** a sample travels as one DATA submessage with a 16-bit length, so
  65,400 data bytes is the most that passes, with a large enough stream
  window and receive buffer; at the defaults a 1 KB sample is refused at
  receive. A sample at or over 64 KiB used to be lost or cut, and is now
  refused at publish (PR #4888). Of the messages above, only a
  `vision_msgs` array of more than about 148 detections, or a `yolo_msgs`
  array of more than about 340, is over the limit; `max_det: 300` with
  `vision_msgs` is.
- **zenoh:** every size above passes on the host once the fragment and
  subscriber buffer sizes exceed the sample. At the defaults, 16 KB is
  dropped at the subscriber buffer, so a 100-detection array needs raised
  limits.

### W2 — Capability matrix

- **Owner:** nros 175.
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

#### W2 results so far (2026-11-26)

The probe is `packages/testing/nros-tests/bins/large-msg`: one program, one
image per RMW and target, each image stating the size knobs it needs. A cell
sends five `std_msgs/UInt8MultiArray` samples and the listener checks every
byte of each. The passing host cells are in `tests/large_msg.rs`
(`large_message_is_delivered_intact_on_the_host`), from the fixture rows
`large-msg-probe-<rmw>`.

**Native POSIX host**, raw (serialized) path:

| Size | zenoh | XRCE | Cyclone |
| --- | --- | --- | --- |
| 1 KB | pass | pass | pass |
| 16 KB | pass | pass | pass |
| 64 KB | pass | fail: protocol limit | pass |
| 256 KB | pass | fail: protocol limit | pass |
| 1 MB | pass | fail: protocol limit | pass |
| 4 MB | pass | fail: protocol limit | pass |

The configuration each needed:

- **zenoh:** `frag_max_bytes` and `subscriber.buffer_bytes` both above the
  largest sample (4,196,352 and 4,195,328 for the 4 MB cell). With the
  defaults, 1 KB passes; 16 KB is dropped at the subscriber's buffer and
  counted (`overflow_drops`); 64 KB and above is dropped past the reassembly
  ceiling (65,535 on POSIX) with nothing counted, while every publish
  returns `Ok`.
- **XRCE:** `subscriber_buffer_bytes` above the sample, and
  `transport_mtu x stream_history` above it too (4096 x 16 for 16 KB). With
  the defaults, 1 KB is refused at receive (`MessageTooLarge`: the 1,024-byte
  ring entry holds the data but not the CDR prefix) and 64 KB fails at
  publish. 64 KB and above fails for any configuration: a sample is one DATA
  submessage, whose length is 16 bits on the wire. Up to 65,400 bytes passes
  with a large enough window. Above that, a sample used to be lost, or
  delivered cut to its size modulo 65,536, while the publish returned `Ok`;
  it is now refused at publish with `MessageTooLarge` (issue 2978).
- **Cyclone:** nothing; the defaults carry 4 MB.

Still to measure: the embedded targets, and the `inline` and `view` storage
modes (the typed path), which the raw path above does not exercise.

### W3 — Memory cost and placement

- **Owner:** nros 175.
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

The core slot pool a device job takes its input in (RFC-0038 D5) is built by
[phase-579](phase-579-device-ports.md) W3p, with two classes whose slot size
and count are knobs. Stating them in the sizing descriptor is phase-579 W3p-2,
which takes its class measurements from here.

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

#### Shape of the verdict

The verdict is filled in once W2 to W5 have measured. It has three parts.

1. **One row per phase criterion.** Each row gives the criterion and its
   threshold, the measured value with the item and target it came from, and
   one of three outcomes:
   - **holds**: measured, and within the threshold;
   - **fails**: measured, and outside it, with the limiting component named;
   - **not measured**: with the reason, such as a target not yet run.
     A criterion that is not measured is never counted as holding.
2. **One row per W1 requirement and RMW.** For each message the real-time
   core receives (size and rate, from the W1 table), and for zenoh, XRCE and
   Cyclone, the row says whether W2's matrix carries it, whether W3's memory
   figure fits an M7-class map, and what configuration it needed. A
   protocol limit, such as XRCE's 16-bit sample length, is recorded as a
   limit of that RMW, not as a defect to fix.
3. **Blocking issues, ranked.** Each defect behind a failed row becomes an
   issue homed here, with its measurement. They are ranked by how many
   applications they block, then by fix cost, and W7 takes them in that
   order.

The overall verdict is **feasible** only if every criterion holds. It is
**feasible with named fixes** if every failure has a W7 issue whose fix is
inside this phase's scope. Otherwise it is **not feasible**, naming the
criterion and the component.

### W7 — Fixes named by W6

- **Owner:** unassigned.
- **Status:** OPEN
- One PR per issue filed in W6, in the order W6 ranks them. Each fix is
  within the scope above; a fix that needs a board-side change ships the
  nano-ros hook and documents what the board package supplies.
- **Acceptance:** every blocking issue from W6 is resolved, and the W2, W4
  and W5 measurements that failed now pass.

### W8 — GPU perception on the original Jetson Nano, with its latency

- **Owner:** nros 241.
- **Status:** BLOCKED (deferred until the board is attached) (user ruling 2026-11-26: the Nano is not attached yet). It runs after phase-589 W10, W12 and W11, on the RT kernel W11 builds. About 30 minutes to build the TensorRT engine on the Nano, then about half a day of measurement with a camera attached: stock and RT kernels, idle and under `stress-ng`.
- **The board:** the Jetson Nano of phase-589 W10, on JetPack 4.6.6 (CUDA
  10.2, TensorRT 8.2, a Maxwell GPU). The Nano runs TensorRT engines in FP16;
  NVIDIA says INT8 is not supported on it. A YOLOv8n engine has been reported
  at about 6 frames per second there, so the measured rate goes in the table
  rather than an assumed one.
- **Two processes, by libc:** CUDA and TensorRT are glibc shared libraries,
  so the detector is a dynamically linked glibc 2.27 C++ node. It is
  cross-built against a sysroot taken from the Nano's L4T root filesystem,
  because the `nros` CLI needs glibc 2.34 and cannot run on the Nano itself.
  The consumer that acts on the detections is the static musl node of
  phase-589 W10, run under `SCHED_FIFO` on the RT kernel of W11.
- **What:** a C++ node (nros-cpp, zenoh) that runs a YOLO-class TensorRT
  engine on camera frames and publishes a detection message per frame. Each
  message carries the frame's capture time and the time inference ended. The
  RT node subscribes and records its receive time. The engine, camera code and
  Nano board package live in the user's project.
- **Measurements:** the detector's frame rate; inference time; and the delay
  from inference end to receipt at the RT node, at p50, p99 and maximum,
  without load and with `stress-ng` load on the other cores, on the stock
  kernel and the RT kernel.
- **Acceptance:** that table in this document. At the detector's own rate,
  the p99 delay from inference end to receipt stays below one frame period
  of the phase's 30 Hz class (33 ms) on the RT kernel under load, or the
  limiting component is named in W6.

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
| [2696](../issues/archived/2696-executor-wake-from-interrupt-not-wired.md) | A device job's completion interrupt cannot wake the executor safely, which the accelerator and camera paths need |
| [2978](../issues/2978-xrce-sample-over-64k-delivered-truncated.md) | W2 measured it: an XRCE sample over 64 KiB publishes without error and is lost or delivered cut, so XRCE's limit has to be refused, not met |
