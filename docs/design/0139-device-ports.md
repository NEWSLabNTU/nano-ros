---
rfc: 0139
title: "Device ports: accelerators, cameras and other job devices under the executor"
status: Draft
area: runtime-api
since: 2026-11
last-reviewed: 2026-11
implements-tracked-by: [phase-579]
amends: []
supersedes: []
superseded-by: null
---

# RFC-0139 — Device ports

**Status:** Draft (2026-11-18). The direction was reviewed with the user on
2026-11-18: a generic device interface rather than an NPU-specific one; kept
out of the ROS-counterpart user API; C as the canonical seam, with device
implementors and device users as two distinct roles. The three questions
raised in that review are settled below (D7, D6, D9). The decisions D1–D10
await the user's review of this text. Tracked by phase-579.

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
