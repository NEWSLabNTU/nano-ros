---
rfc: 0139
title: "Device ports: accelerators, cameras and other job devices under the executor"
status: Accepted
area: runtime-api
since: 2026-11
last-reviewed: 2026-11
implements-tracked-by: [phase-579]
amends: []
supersedes: []
superseded-by: null
---

# RFC-0139 — Device ports

**Status:** Accepted (2026-11-19, user ruling). Drafted 2026-11-18. The direction was reviewed with the user on
2026-11-18: a generic device interface rather than an NPU-specific one; kept
out of the ROS-counterpart user API; C as the canonical seam, with device
implementors and device users as two distinct roles. The three questions
raised in that review are settled below (D7, D6, D9). The user accepted the
decisions on 2026-11-19. Tracked by phase-579.

**Amendment (2026-11-26, DRAFT): the Rust shape**, both ends, with four questions
for the user at its top (see [the amendment](#amendment-2026-11-26-draft-the-rust-shape)).

## Summary

Hardware that does work on its own and reports completion — a neural
processing unit (NPU), a GPU, a 2D image engine, a JPEG codec, a DSP reached
over inter-core messaging, a camera, an IMU FIFO — is described by one model:

- a **worker** takes a job and completes it;
- a **source** produces data on its own clock.

A device's completion becomes an executor event, and the work that follows it
runs as an ordinary callback at the chain's priority. The executor thread is
never held while a device runs.

The design is split into five layers. The ROS-counterpart user API (`rclc`,
`rclcpp`, `nros`) is **unchanged**: it takes only names stock ROS 2 has
(RFC-0089). Device support lives below it, in a C port ABI that board
packages implement, and beside it, in an opt-in companion package that node
authors use.

## Motivation

Perception chains (camera, pre-processing, inference, post-processing,
control) are the workload behind the drone and sprayer applications
(phase-574). Measured against today's tree:

- **A device callback holds the executor.** Every vendor driver studied can
  split submission from completion, but its default glue blocks: Arm's
  Ethos-U driver waits with `WFE`, ST's Neural-ART runtime calls its OS
  layer's `WFE` (reported on the vendor forum to starve other FreeRTOS tasks),
  TensorFlow Lite Micro's `Invoke` returns only when inference ends. A callback
  that calls them stalls every other callback on its executor for the whole
  inference.
- **Completion cannot reach the executor from an interrupt.** Issue 2696:
  `GuardCondition::trigger()` reaches `xSemaphoreGive` on FreeRTOS, which is
  not safe in an interrupt handler, and the interrupt-safe platform signal has
  no caller.
- **A received buffer cannot outlive its callback.** A camera frame handed to
  an NPU job must, and today it must be copied.
- **Nothing states device time.** A chain's latency budget cannot include
  waiting for a shared, non-preemptive accelerator, because no contract names
  one.

### What real devices look like

| Device | RTOS interface | Linux interface | How completion is reported |
| --- | --- | --- | --- |
| Arm Ethos-U55/U65/U85 | `ethosu_invoke_async` / `ethosu_wait` | DRM accel driver (posted upstream 2025) | interrupt |
| ST Neural-ART | `LL_ATON_RT_RunEpochBlock` loop | — | epoch interrupt |
| NXP eIQ Neutron | a TFLite Micro operator | — | blocking `Invoke` |
| TI C7x with matrix accelerator | TIOVX graph node | TIOVX graph | graph completion |
| ST DMA2D, NXP PXP, JPEG codecs | HAL `Start_IT` / `_DMA` calls | V4L2 memory-to-memory | HAL callback from the interrupt |
| NVIDIA GPU / DLA | — | TensorRT `enqueueV3`, CUDA event | event; DLA is synchronous |
| Camera | Zephyr `video_enqueue`/`video_dequeue`, `video_set_signal` | V4L2 `QBUF`/`DQBUF`, `poll` | poll signal or file descriptor |
| IMU FIFO, ADC burst | Zephyr RTIO sensor streaming, `adc_read_async` | IIO | RTIO completion, poll signal |

Two scheduling properties hold across them: device jobs are **non-preemptive**
(Neural-ART's epoch boundaries and TIOVX graph nodes are the exceptions), and
inference time on a dedicated processor is **nearly constant** (one measured
pose model: 18.4 ± 0.3 ms). The variance that remains comes from memory
traffic, CPU post-processing and waiting behind another job on the same device.

## Design

### D1 — Five layers, and the rule that places them

| Layer | Contents | Written by |
| --- | --- | --- |
| 4. Launch and contracts | device and stage declarations, checker rules (D10) | integrators |
| 3. ROS user API | **unchanged**: `rclc`, `rclcpp`, `nros` | every node author |
| 2. Companion package `nros-device` | stages, queue policies, data age, slot views, the blocking wrapper, per-class job types (D7) | node authors who use devices |
| 1. Executor extension hook | a static device event source, interrupt-safe completion, priority binding, owned slots (D4, D6) | core only; called by layer 2 |
| 0. Device ports | the C port ABI (D3) and its adapters | board packages, vendors |

The rule: **the ROS user API takes only names stock ROS 2 has.** A capability
stock ROS lacks goes in a companion package built on a narrow, documented
hook, never on the mirrored types. Stock ROS has no device API in its client
libraries (hardware lives in `ros2_control`, `image_transport`, vendor
transports), and RFC-0089 declines `Waitable`, the one extension point rclcpp
has for custom event sources, because dynamic wait-set membership needs an
allocator. The precedent for a companion package is `nros-diagnostic-updater`.

**Who provides what.** Board specifics stay with the board package, on the
user's side (RFC-0135); nano-ros ships only what is the same for every device.

| Layer | nano-ros provides | A board package provides |
| --- | --- | --- |
| 0. Device ports | the ABI header `<nros/device_port.h>`, its Rust and C++ views, the layout checks, the mock ports (D11), and the adapters whose APIs are an OS's own (Zephyr video and RTIO, Linux V4L2; D9) | the port **implementation** for its hardware: the ops table, the interrupt handler that signals completion, and the vendor SDK, named by the package's own variable |
| 1. Executor hook | all of it | nothing |
| 2. Companion package | all of it, including the per-class job types | nothing; a board may ship a typed handle for its device (`acme::npu0`) as a convenience |
| 3. ROS user API | unchanged | nothing |
| 4. Contracts | the schema and the checker | the device's measured job times, stated in its board definition, as other board facts are |

So a board package that adds an NPU ships one C file (or C++, or Rust) and
a few lines of board definition. It never patches nano-ros, and nano-ros
never carries a vendor's SDK.

Consequence for portability: a node keeps its ROS logic in one component and
its device stages in another. The ROS component still compiles against stock
rclcpp; the device component is a nano-ros extension and says so.

### D2 — Two roles of device, and when not to use either

- **Worker:** a request goes in, a result comes back (NPU, GPU, DLA, 2D
  engine, codec, crypto engine, a remote DSP).
- **Source:** produces on its own clock (camera, IMU FIFO, ADC burst, radar
  front end). A source is to hardware what a subscription is to the network.

A device that is used once inside one callback, with a short job and no
sharing, stays a plain driver call. The device model is for devices that are
shared, long-running, or part of a timed chain; a job much shorter than a
callback (tens of microseconds) costs more as a stage than as a wait.

### D3 — The port ABI: one C header, the source of truth

`<nros/device_port.h>` is a core ABI header, beside `<nros/rmw_transport.h>`
(the custom-transport vtable is the precedent). It is authored as the source
of truth; Rust consumes committed bindgen output and C++ a header-only view
(RFC-0054). Layout equivalence is checked on both sides.

```c
#define NROS_DEVICE_PORT_ABI_V1 ((uint32_t)1)

typedef struct nros_completion_s { void *_slot; } nros_completion_t;
void nros_completion_signal(nros_completion_t c);            /* thread context */
void nros_completion_signal_from_isr(nros_completion_t c);   /* the only call allowed in an ISR */

typedef struct nros_worker_port_ops_s {
    uint32_t abi_version;          /* NROS_DEVICE_PORT_ABI_V1 */
    uint32_t _reserved;
    void *user_data;
    nros_device_ret_t (*submit)(void *ud, const nros_device_job_t *job, nros_completion_t done);
    nros_device_ret_t (*take)(void *ud, nros_device_result_t *out);
    nros_device_ret_t (*cancel)(void *ud);          /* may be NULL */
} nros_worker_port_ops_t;

typedef struct nros_source_port_ops_s {
    uint32_t abi_version;
    uint32_t _reserved;
    void *user_data;
    nros_device_ret_t (*start)(void *ud, nros_completion_t ready);
    nros_device_ret_t (*take)(void *ud, nros_device_result_t *out);
    void (*stop)(void *ud);
} nros_source_port_ops_t;
```

- **`submit` never blocks**, and the runtime calls it only when the device is
  idle. **`take` never blocks**; it returns `NROS_DEVICE_RET_PENDING` when the
  device is not finished.
- **The job and result are type-erased** at this layer: a job is an operation
  handle plus input and output slots; a result is a slot, a length, a capture
  time and a status. Typed views are the companion package's (D7).
- **The interrupt rule:** `nros_completion_signal_from_isr` is the only
  function a port may call from an interrupt handler. This differs from the
  custom transport, whose contract forbids interrupt context entirely; devices
  complete in interrupt handlers, so the device ABI names the one safe call.
  It depends on issue 2696.
- **A port never sees the executor.** It holds only completion handles, so a
  vendor adapter cannot affect scheduling.
- **Versioning** follows the custom transport: `abi_version`, a reserved word,
  appended fields only.
- **Completion by handle (park):** a port whose completion is a file
  descriptor (Linux V4L2, eventfd, DRM fences) or a Zephyr `k_poll_signal`
  may instead state that handle, and the executor's park waits on it, the
  mechanism RFC-0106 already uses for transport sockets.

### D4 — The executor extension hook (layer 1)

- **A static stand-in for `Waitable`.** A device event source is registered
  before the first spin, into a fixed slot (the existing callback slots count
  it), with no allocation. That keeps RFC-0089's reason for declining
  `Waitable` intact.
- **A new entry kind, `Device`,** beside `GuardCondition` in the executor's
  arena. A completion sets the entry's ready bit through the attributed-wake
  path the entity wakers already use (`WakeCtx::mark_ready`), then wakes the
  spin.
- **Priority** comes from the sched context the entry is bound to: its bucket
  (critical, normal, best-effort) and its deadline in the bucket's EDF order.
- **LET mode** samples device completions at the start of the spin, as it
  samples subscriptions.
- **Several executors:** a completion targets the executor that owns the
  stage, as a subscription does.

The hook is public in the sense that the companion package and board code
call it; it is not part of the ROS user API and is documented as an extension
surface.

### D5 — One queue per device, and what it guarantees

- The executor owns one software queue per worker device. Jobs from different
  stages on the same device are ordered by the submitting chain's priority.
- **Non-preemptive.** A running job is never interrupted. A waiting job's
  delay is bounded by the longest other job on the device plus the jobs ahead
  of it in priority order. Where a device has hardware priority (DMA channel
  priority, GPU stream priority), the chain's priority is mapped onto it.
- **Queue policy per stage:** `KeepLatest` (a newer input replaces a waiting
  older one, the right policy for frames) or `Fifo { depth }`.
- **Capture time travels with the data.** Every result carries the time its
  original input was captured, unchanged through any number of stages. A
  result older than the stage's `max_age` is dropped and reported, never
  handed on.
- **Trace:** each stage records submit, device start, device done and
  continuation start, beside the existing per-callback trace.

### D6 — Owned slots

A device job needs a buffer that outlives the callback that produced it.

- **Core (layer 1):** an owned handle to a slot of the large-message pool,
  returned to its pool when released. Moving it into a job transfers
  ownership.
- **C:** `nros_slot_t`, with `nros_slot_move()`, which empties the source.
  Using an emptied slot asserts in debug builds. (Question 2 of the review:
  kept, because a use after handover is the most likely C error and the empty
  state makes it detectable.)
- **C++ and Rust:** move-only types, so the compiler enforces the handover.
- **Views** (a frame descriptor, a tensor view) are the companion package's.

### D7 — The companion package `nros-device` (layer 2)

One package in three languages: the crate `nros-device`, the CMake target
`NanoRos::Device`, the headers `<nros/device.h>` and `<nros/device.hpp>`. It
is not under `rclc/` or `rclcpp/`.

It provides worker and source registration, stages, queue policies, data-age
handling, slot views and the blocking wrapper (D8). It also holds the
**per-class job and result types** — inference, 2D operation, camera frame,
sensor batch — each in a small header of its own. The core ABI keeps only the
generic job and result. (Question 1 of the review.)

### D8 — Drivers that can only block

`BlockingWorker` wraps a driver whose only call blocks (TFLite Micro `Invoke`,
the DLA). It runs the call on one task of its own at the stage's priority and
signals completion when the call returns. The executor sees the same submit
and completion as for an asynchronous device. It costs one task stack; the
analysis treats the device time as suspension, as for a real asynchronous
device.

### D9 — Where adapters live

| Adapter | Home |
| --- | --- |
| Vendor accelerators and engines (Ethos-U, Neural-ART, Neutron, DMA2D, PXP, JPEG) | board packages (RFC-0135), with the vendor SDK named by the package's own variable |
| Zephyr video, Zephyr RTIO sensors, Zephyr ADC | in-tree, in the Zephyr platform layer: the APIs are Zephyr's own |
| Linux V4L2 and eventfd | in-tree, in the native platform layer |
| TensorRT and CUDA | an optional package, not built by default (question 3 of the review) |
| TI TIOVX graph | the remote side runs the graph; the nano-ros core on the R5F sees one remote worker whose completion arrives over the inter-core link (phase-575, phase-576) |

On Zephyr, a port may be a thin layer over RTIO: RTIO is a submission queue
and a completion queue, the same shape as D3.

### D10 — Contracts and analysis (layer 4)

The image contract gains device and stage declarations beside its rates, data
ages and path budgets:

```yaml
devices:
  npu0:    { kind: worker, preemptive: false }
  camera0: { kind: source, rate_hz: 30, buffers: 3 }
stages:
  - { node: perception, stage: detector, device: npu0,
      cpu_before_us: 400, device_us: 9000, cpu_after_us: 1200,
      priority: critical, queue: keep_latest, max_age_ms: 50 }
paths:
  - { from: camera0, to: /control/cmd, max_latency_ms: 80 }
```

The checker:

- sums each device's utilisation against its capacity;
- adds the non-preemptive blocking term (the longest other job on the device);
- bounds each chain with a suspension-aware analysis, each device queue as a
  separate resource. A published analysis is reused rather than derived,
  because the self-suspension literature has a record of flawed bounds.

Device times are measured, not assumed, as stack floors are, and the trace
(D5) reports a stage that exceeds its declared time.

The schema and the checker belong to the launch toolchain and are agreed with
its owner.

### D11 — Testing without the hardware: mock ports

nano-ros ships one mock worker and one mock source, as RFC-0132 ships the
CAN `mock` backend, so a node's stages and a contract's timing are tested
with no device at all:

- **Mock worker:** completes each job after a configured time, from a timer
  interrupt on an RTOS image and from a thread on a host, with an output the
  test supplies (or a copy of the input). It can inject a fault, a timeout and
  a configured spread in job time, so the drop, timeout and queue paths are
  exercised.
- **Mock source:** delivers queued buffers at a configured rate, each stamped
  with its capture time, or a buffer per call when the test drives it.
- **Where they run:** host-process boards and the emulator images that the
  test lanes already boot (MPS2 under QEMU, Zephyr `native_sim`). The same
  node binary runs against the mock or the real port; which one is the board
  definition's choice, not the node's.
- **What they do not prove:** the vendor adapter itself. That is tested
  against the vendor's own emulator where one exists (the Ethos-U55 on the
  Corstone-300 FVP, phase-579 W7), and otherwise on hardware and marked
  manual in the support registry (RFC-0114). As in RFC-0132, an emulator
  counts as hardware: a mock never stands in for a device an emulator models.

A board package's adapter is tested the way nano-ros tests its own: the
companion package's conformance tests (submit never blocks, `take` before
completion returns pending, completion from the interrupt wakes the
executor) run against any port, real or mock.

## Examples

**An implementor's adapter, in C (board package):**

```c
static struct { struct ethosu_driver *drv; nros_completion_t done; } npu0;

static nros_device_ret_t npu_submit(void *ud, const nros_device_job_t *job, nros_completion_t done) {
    npu0.done = done;
    return ethosu_invoke_async(npu0.drv, job->cmd, job->cmd_len, job->base_addrs, job->base_sizes,
                               job->n_bases, NULL) == 0 ? NROS_DEVICE_RET_OK : NROS_DEVICE_RET_REJECTED;
}
static nros_device_ret_t npu_take(void *ud, nros_device_result_t *out) {
    int r = ethosu_wait(npu0.drv, false);
    if (r == 1) return NROS_DEVICE_RET_PENDING;
    out->status = (r == 0) ? NROS_DEVICE_RET_OK : NROS_DEVICE_RET_FAULT;
    return NROS_DEVICE_RET_OK;
}
void NPU0_IRQHandler(void) {
    ethosu_irq_handler(npu0.drv);
    nros_completion_signal_from_isr(npu0.done);
}
const nros_worker_port_ops_t acme_npu0_ops = {
    NROS_DEVICE_PORT_ABI_V1, 0, &npu0, npu_submit, npu_take, NULL };
```

**A user's node, in C:**

```c
#include <rclc/rclc.h>
#include <nros/device.h>
#include <nros/device/inference.h>

static nros_device_t npu;  static nros_stage_t det;  static nros_source_t cam;

static void on_detections(const nros_stage_result_t *r, void *ctx) {
    detections_msg_t msg;
    decode_nms(nros_slot_data(&r->output), &msg);
    msg.stamp = r->captured_at;
    rcl_publish(&pub_det, &msg, NULL);
}

static void on_frame(nros_slot_t *frame, int64_t captured_at, void *ctx) {
    nros_inference_job_t job = { .model = &detector, .input = nros_slot_move(frame) };
    nros_stage_submit(&det, &job, captured_at);
}

void app_setup(rclc_executor_t *exec) {
    nros_device_register_worker(exec, &acme_npu0_ops, &npu);
    nros_stage_options_t o = NROS_STAGE_OPTIONS_CRITICAL(9000 /* us */, 50000 /* max age, us */);
    nros_stage_init(&det, &npu, &o, on_detections, NULL);
    nros_source_init(&cam, exec, &zephyr_camera0_ops, &NROS_SOURCE_OPTIONS_KEEP_LATEST, on_frame, NULL);
}
```

**The same node, in C++:**

```cpp
#include <rclcpp/rclcpp.hpp>
#include <nros/device.hpp>

nros::device::Worker npu{executor, acme::npu0};
nros::device::Stage<InferenceJob, InferenceOutput> det{npu, nros::device::critical(9ms, 50ms),
    [&](nros::device::Result<InferenceOutput>&& out) {
        auto msg = decode_nms(out.value());
        msg.header.stamp = out.captured_at();
        pub->publish(msg);
    }};
nros::device::Source<Frame> cam{executor, zephyr::camera0, nros::device::keep_latest(40ms),
    [&](nros::device::Slot<Frame>&& frame, nros::Time t) {
        det.submit({&detector, std::move(frame)}, t);
    }};
```

**A C++ implementor** derives from `nros::device::WorkerPort<Self, Job,
Output>`, which builds the vtable; the header uses no STL, so it is
freestanding-safe. **A Rust implementor** implements the matching trait, whose
vtable is the same ABI.

## Amendment (2026-11-26, DRAFT): the Rust shape

**Status: DRAFT, awaiting the user's ruling on Q1–Q4 below.** D1 promises a
Rust view of the port ABI and D7 a Rust crate, and the Examples end with "a
Rust implementor implements the matching trait"; nothing above says what
either looks like. This amendment gives both ends, the node API (R2, R3) and
the implementor trait (R5, R6), to match the C and C++ shapes. Nothing here
changes D1–D11 or the C and C++ shapes. Implementation (phase-579 W4) starts
after the ruling.

### Questions for the user

- **Q1. One Rust user tier or two?** A Rust node is written as a component:
  it registers entities in `register(ctx)`, names each callback with a string,
  handles them in `on_callback`, and sends by name (`send_goal_for_name`). The
  C++ example instead holds handles and passes closures. Proposed: the
  **component tier (R2) is the Rust user surface**, and a **closure tier over
  the executor (R3)** serves an image with no component, as guard conditions
  have both a node form and an executor form. Alternative: the component tier
  only, with the closure tier added when an image needs it.
- **Q2. How `nros-device` reaches a component's context without adding names
  to the mirrored types.** The component tier must register a stage from
  `register(ctx)`, read a stage result inside `on_callback`, and submit from
  `on_callback` or `tick`. Proposed: **(a) free functions in `nros-device`
  that take the context** (`nros_device::submit_for_name(ctx, ...)`), reached
  through one documented extension accessor that `NodeContext`, `CallbackCtx`
  and `TickCtx` expose for companion packages. That accessor is D4's hook
  surfaced to the component tier, and the phase's "no new name in the user
  API" gate exempts it by name. Alternative: **(b) an extension trait**
  (`use nros_device::prelude::*;` then `ctx.submit_for_name(...)`). It is
  shorter, but it reads as a method on the mirrored type, and the gate could
  not tell it from one.
- **Q3. How a board names a device handle.** Proposed: a board crate exports
  typed statics in a `devices` module, Rust's spelling for a constant:
  `acme_board::devices::NPU0`. The C ops table stays `acme_npu0_ops` and the
  C++ handle `acme::npu0`, so one device has three spellings, one per
  language's convention. Alternative: a lowercase function,
  `acme_board::devices::npu0()`, the same word as C++ but a call.
- **Q4 (all three languages). Which clock is the capture time on?** D5 says it
  travels with the data, and `max_age` is measured against it. The examples
  write it into `header.stamp`, which is the wall clock. Under RFC-0141 the wall
  clock can step (an SNTP re-sync), and an age computed across a step is
  wrong. Proposed: the capture time is **monotonic**, which `max_age` uses.
  Each result also carries the **wall-clock reading taken at the same
  instant**, which `stamp()` returns for a message header. C and C++ gain the
  same pair (`captured_at` stays monotonic; a `captured_stamp` field is
  added). Alternative: wall clock only, and accept a wrong age across a
  step.

### R1 — The crate, and the no_std rules it keeps

- `nros-device` is `#![no_std]` and needs no `alloc`, like the core crates. It
  builds wherever `nros-node` builds. Edition 2024: `unsafe extern "C"`,
  `#[unsafe(no_mangle)]`, an explicit `unsafe {}` in every `unsafe fn`.
- **Storage** is a `static` (a port's state, a device handle) or the executor's
  arena (a worker, a stage, a source, sized by the generated sizes header, as
  the Rules below require). Registration happens before the first spin; a later one
  returns an error and changes nothing.
- **The ABI types** are W2's committed bindgen output, re-exported as
  `nros_device::sys`. The crate never hand-mirrors a port struct (the
  hand-mirrored-struct drift class), and its layout checks are W2's.
- **Errors:** `DeviceError` (`Rejected`, `Fault`, `Timeout`, `Unsupported`,
  `AbiMismatch`) maps one to one onto `nros_device_ret_t`. "Not finished yet" is
  `core::task::Poll::Pending`, not an error. The user tier returns the
  `NodeResult` the rest of a component returns.
- **Diagnostics** go through `nros_log`, never `std` stdio (it is fatal on
  native_sim).
- **Typed classes:** a per-class module (`inference`, `image2d`, `camera`,
  `sensor`) defines a class marker (`Inference`) and its job and output types.
  A class is a trait, `JobClass { type Job; type Output; }` for a worker and
  `SourceClass { type Item; }` for a source. It converts between the typed
  view and the type-erased job and result of D3. The type erasure stays at
  the ABI, as D7 says.

### R2 — The node API, component tier

The C++ example's node, as a Rust component:

```rust
#![no_std]
use core::time::Duration;
use nros::{Callback, CallbackCtx, Component, ExecutableNode, NodeContext, NodeOptions, NodeResult};
use nros_device::{camera::Frame, inference::{Inference, InferenceJob}, SourceOptions, StageOptions};

pub struct Perception;

impl Component for Perception {
    const NAME: &'static str = "perception";

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let _node = ctx.create_node(NodeOptions::new("perception"))?;
        let npu = nros_device::worker(ctx, &acme_board::devices::NPU0)?;
        nros_device::stage::<Inference>(ctx, npu, "detector",
            StageOptions::critical(Duration::from_millis(9), Duration::from_millis(50)),
            "on_detections")?;
        nros_device::source::<Frame>(ctx, &nros_zephyr_devices::CAMERA0, "camera",
            SourceOptions::keep_latest(Duration::from_millis(40)), "on_frame")?;
        Ok(())
    }
}

impl ExecutableNode for Perception {
    type State = ();
    fn init() {}

    fn on_callback(_: &mut (), callback: Callback<'_>, ctx: &mut CallbackCtx<'_>) {
        match callback.as_str() {
            "on_frame" => {
                if let Ok(frame) = nros_device::take::<Frame>(ctx) {
                    let job = InferenceJob { model: &DETECTOR, input: frame.slot };
                    let _ = nros_device::submit_for_name::<Inference>(ctx, "detector", job, frame.captured);
                }
            }
            "on_detections" => {
                if let Ok(out) = nros_device::result::<Inference>(ctx) {
                    let mut msg = decode_nms(out.output().data());
                    msg.header.stamp = out.stamp();
                    // publish `msg` as the node publishes anything else
                }
            }
            _ => {}
        }
    }
}
```

- **Registration** (`worker`, `stage`, `source`) takes the context, a device
  handle (R5) and a name; a stage and a source also take options and the name
  of the callback that receives their results. `worker` returns a
  `WorkerId`, which is `Copy` and borrows nothing, so the component holds no
  lifetime.
- **Reading** inside `on_callback` mirrors `ctx.message::<M>()`: `take::<T>`
  yields a source's item and `result::<C>` a stage's result. Each returns
  `Err` when the callback being dispatched is not that kind.
- **Submitting** by stage name mirrors `send_goal_for_name`, and works from
  `on_callback` or `tick`. It takes ownership of the job's slots (R4), and on
  `Err` hands them back inside the error, so no buffer is lost.
- **Options** are consts with the C++ spellings: `StageOptions::critical(device
  time, max age)` / `::normal` / `::best_effort`, and `SourceOptions::keep_latest(max
  age)` / `::fifo(depth, max age)`.

### R3 — The node API, closure tier (Q1)

For an image with no component (a bare `Executor::open` plus devices), the
same objects are opened on the executor with closures, as
`register_guard_condition` is:

```rust
let npu = nros_device::Worker::open(&mut executor, &acme_board::devices::NPU0)?;
let det = nros_device::Stage::<Inference>::open(&mut executor, npu,
    StageOptions::critical(Duration::from_millis(9), Duration::from_millis(50)),
    |out: nros_device::StageResult<Inference>| { /* decode, publish */ })?;
nros_device::Source::<Frame>::open(&mut executor, &nros_zephyr_devices::CAMERA0,
    SourceOptions::keep_latest(Duration::from_millis(40)),
    move |frame: nros_device::Item<Frame>| { let _ = det.submit(InferenceJob { model: &DETECTOR, input: frame.slot }, frame.captured); })?;
```

The closures are `FnMut + 'static`, the bound every executor callback has. A
`Stage<'s, C>` handle is `Copy` and borrows the executor's backing for `'s`,
as `GuardCondition<'s>` does (the backing-lifetime rule). So a closure can
capture one only when the backing is `'static`, which the default `.bss`
executor backing is.

### R4 — Owned slots and views (D6)

- `Slot<T>` is the owned handle to a large-message pool slot: move-only (no
  `Clone`, no `Copy`), `Send`, released to its pool on `Drop`. Moving it into
  a job is the handover. The compiler rejects a use after the move, which is
  D6's C++ and Rust rule; there is no empty state to assert on.
- `Slot<T>` derefs to the class's view (`Frame` has width, height, format and
  stride; a tensor view has shape and element type). `data()` is the bytes.
- An item from a source is `Item<T> { slot: Slot<T>, captured, stamp }`
  (captured and stamp as Q4 rules).

### R5 — Device handles, and how a board names one (Q3)

A board crate exports one typed static per device:

```rust
pub mod devices {
    pub static NPU0: nros_device::WorkerHandle<nros_device::inference::Inference> = /* R6 */;
}
```

- `WorkerHandle<C: JobClass>` and `SourceHandle<C: SourceClass>` are
  `#[repr(transparent)]` over `&'static` the D3 ops table, with the class as
  a type parameter. A stage of another class cannot be bound to the device;
  that is a compile error, not a runtime refusal.
- Registration checks `abi_version` and returns `DeviceError::AbiMismatch`
  for a table it does not speak, so a stale adapter fails at registration and
  never at a job.
- **A device whose adapter is C** is named from Rust with one `unsafe` line,
  in which the board asserts the C table's class:

  ```rust
  unsafe extern "C" { static acme_npu0_ops: nros_device::sys::nros_worker_port_ops_t; }
  pub static NPU0: WorkerHandle<Inference> = unsafe { WorkerHandle::from_c(&acme_npu0_ops) };
  ```

- The in-tree adapters of D9 (Zephyr video and RTIO, Linux V4L2) are named the
  same way, from the platform layer's own module (`nros_zephyr_devices::CAMERA0`
  above, the crate name to be settled with W8).

### R6 — The implementor trait

A Rust adapter implements a trait. The crate builds the D3 ops table from it,
so an adapter written in Rust serves a C, C++ or Rust node (phase-579 W4's
acceptance):

```rust
pub trait WorkerPort: Send + 'static {
    type Class: JobClass;
    /// Never blocks; called only when the device is idle (D3).
    fn submit(&mut self, job: &<Self::Class as JobClass>::Job, done: Completion) -> Result<(), DeviceError>;
    /// Never blocks; `Pending` while the device is still working.
    fn take(&mut self, out: &mut ResultSink<'_, Self::Class>) -> Poll<Result<(), DeviceError>>;
    /// Whether `cancel` is implemented; when false the table's `cancel` is NULL.
    const CAN_CANCEL: bool = false;
    fn cancel(&mut self) -> Result<(), DeviceError> { Err(DeviceError::Unsupported) }
}

pub trait SourcePort: Send + 'static {
    type Class: SourceClass;
    fn start(&mut self, ready: Completion) -> Result<(), DeviceError>;
    fn take(&mut self, out: &mut ResultSink<'_, Self::Class>) -> Poll<Result<(), DeviceError>>;
    fn stop(&mut self);
}
```

The Ethos-U adapter of the Examples, in Rust:

```rust
static NPU: PortCell<AcmeNpu> = PortCell::new(AcmeNpu::new());
static DONE: CompletionCell = CompletionCell::new();

impl WorkerPort for AcmeNpu {
    type Class = Inference;
    fn submit(&mut self, job: &InferenceJob, done: Completion) -> Result<(), DeviceError> {
        DONE.set(done);
        // SAFETY: the driver handle is this port's; ethosu_invoke_async does not block.
        match unsafe { ethosu_invoke_async(self.drv, job.command(), /* bases */) } {
            0 => Ok(()),
            _ => Err(DeviceError::Rejected),
        }
    }
    fn take(&mut self, out: &mut ResultSink<'_, Inference>) -> Poll<Result<(), DeviceError>> {
        // SAFETY: as above; `false` makes ethosu_wait return at once.
        match unsafe { ethosu_wait(self.drv, false) } {
            1 => Poll::Pending,
            0 => Poll::Ready(out.complete()),
            _ => Poll::Ready(Err(DeviceError::Fault)),
        }
    }
}

#[unsafe(no_mangle)]
extern "C" fn NPU0_IRQHandler() {
    // SAFETY: the vendor's handler for this interrupt.
    unsafe { ethosu_irq_handler(NPU.driver()) };
    DONE.signal_from_isr();
}

pub static NPU0: WorkerHandle<Inference> = WorkerHandle::new(&NPU);
/// The same device for a C or C++ node.
#[unsafe(no_mangle)]
pub static acme_npu0_ops: WorkerPortOps = WorkerPortOps::of(&NPU);
```

- **`PortCell<P>`** holds a port's state in a `static`. It is `Sync` by
  `unsafe impl`, on D3's guarantee that the runtime calls one port's
  thread-context functions only from the executor that owns it, one at a time.
  It is the only place the crate asserts that.
- **The ops table** is built in a `const fn` from `extern "C"` trampolines
  generic over the port, so it is a plain `static`, with no constructor and
  nothing to call at boot. `WorkerPortOps` wraps the bindgen struct
  `#[repr(transparent)]` so a `static` of it can hold `user_data`.
- **`Completion`** is `Copy` and `Send`: `signal(self)` in thread context, and
  `signal_from_isr(self) -> bool` from an interrupt. The bool is whether the
  executor's wake object took the signal, the same answer
  `GuardCondition::trigger_from_isr` gives. **`CompletionCell`** stores one
  atomically, so an interrupt handler can reach the handle `submit` received,
  with no lock.
- The interrupt rule of D3 holds by convention, as in C. Only
  `signal_from_isr` may run in an interrupt, and every other call asserts thread
  context in a debug build. The type system cannot express this, so the
  conformance tests (D11) check it.
- A port implemented in Rust passes the same conformance tests as a C one:
  `nros_device::conformance::worker(&NPU0)` and `::source(...)` run D11's
  checks against any handle.

### R7 — What this adds, and where

| Name | Where | Notes |
| --- | --- | --- |
| `nros-device` crate: registration, `Slot`, handles, traits, per-class modules | companion package (layer 2) | none of it under `nros` |
| the extension accessor on `NodeContext` / `CallbackCtx` / `TickCtx` | the user API's types, as D4's hook | Q2; exempted by the user-API gate by name |
| `nros_device::sys` | W2's bindings, re-exported | generated, never hand-written |

## Rules common to both roles

| Concern | Rule |
| --- | --- |
| Source of truth | `<nros/device_port.h>`; Rust and C++ views are checked against its layout |
| Interrupt context | only `nros_completion_signal_from_isr`; every other call is thread context, asserted in debug builds |
| Storage | static and opaque, sized by the generated sizes header; no `malloc` |
| Registration | before the first spin, into fixed slots |
| Errors | `nros_device_ret_t` at the port ABI; `rcl_ret_t` at the C user API, like the rest of it |
| Versioning | `abi_version`, a reserved word, appended fields only |
| Languages | any adapter serves any user language, because the ABI is the one seam |

## Prerequisites

1. Issue 2696: an interrupt-safe executor wake on every RTOS port.
2. An owned slot handle (D6); today a received buffer is only borrowed.
3. The `Device` entry kind (D4).

## Alternatives considered

- **Methods on `nros::Executor` / `rclc_executor_t`.** Rejected: names stock
  ROS does not have, on types that mirror it (RFC-0089).
- **Adopt `Waitable`.** Rejected for the reason RFC-0089 gives: dynamic
  membership needs an allocator. D4 is its static equivalent.
- **An NPU-specific API.** Rejected: the 2D engine, codec, camera and remote
  DSP have the same shape, and a second model for each would be a second
  spelling of one mechanism.
- **An untyped job blob in the user API.** Rejected: type erasure stays at the
  ABI; users get typed views.
- **A Linux GPU arbiter for third-party rclcpp nodes.** Out of scope here: a
  node that calls TensorRT directly bypasses any arbiter. Its contract can be
  stated and measured, nothing more.

## Non-goals

- Loading or running models. That is the vendor runtime's (Vela, Neural-ART,
  TIDL, TFLite Micro, TensorRT).
- Camera formats, ISP tuning, codec parameters: the platform's class APIs.
- Memory-bandwidth regulation between CPU and accelerator (MemGuard-style).
  It affects device time, and the contract measures it; regulating it is
  separate work.

## Related

- RFC-0089 (ROS 2 API adoption), RFC-0054 (C headers as the ABI source of
  truth), RFC-0106 (executor park), RFC-0135 (board packages), RFC-0038 and
  RFC-0010 (zero-copy and loans), RFC-0050 (system model and contracts).
- Phase-574 (perception and large messages), phase-575 and phase-576 (the
  inter-core link a remote worker completes over).
- Issue 2696 (interrupt-safe wake).

## Changelog

- 2026-11-18: drafted.
- 2026-11-18: who provides what per layer (D1), and mock ports (D11).
- 2026-11-19: ACCEPTED by the user's ruling of 2026-11-19.
- 2026-11-26: amendment drafted: the Rust shape (R1-R7) and questions Q1-Q4,
  awaiting the user's ruling.
