# Production Readiness Checklist

A copy-out checklist for teams piloting nano-ros toward production
deployment. Each box is a concrete validation step, not a marketing
claim. The book documents the framework's intent + plumbing; this
page is what **your team** needs to confirm on **your target** before
shipping.

> **Why a separate checklist?** nano-ros is production-capable, but
> some acceptance items are hardware-gated (P99 latency on real
> Cortex-M3, multicast on real silicon, NuttX SCHED_SPORADIC under
> kernel config) and can't be validated in CI. The checklist gives
> you the steps to close those gaps for your deployment.

## 1. Real-time metrics (hardware-validated)

The book's quoted poll-WCET / P99-latency numbers come from QEMU.
DWT cycle counters are best-effort under emulation. **For
production claims, re-measure on your actual silicon.**

- [ ] **End-to-end P99 latency** (publisher → executor callback)
      on your target MCU at its production clock + load. Target:
      ≤ design budget. Tooling: `wake-latency-cortex-m3` bench at
      `packages/testing/nros-bench/wake-latency-cortex-m3/`.
- [ ] **Worst-case stack depth** per task. Tool:
      `cargo-call-stack`, `cargo-stack-sizes`, or the ARM
      stack-analyzer for C/C++.
- [ ] **Heap fragmentation pattern** over 24 h at nominal load.
      Spot-check with `mallinfo` or your RTOS's heap-stats API.
- [ ] **Wake latency** (transport-rx interrupt → first user
      callback dispatched). Required if you use the async / poll-
      blocked spin path.
- [ ] **Spin-loop budget overrun rate** under sustained pub load.
      `Executor::spin_once(timeout)` returns the overrun count.

## 2. Platform-specific validation

Per-RTOS gaps that the book documents as "tested in CI" cover
**reference boards** — your actual board / kernel-config combination
may differ.

- [ ] **Multicast / IGMP** if using DDS (RTPS). Confirm SPDP
      discovery actually fires on your RTOS + driver. Untested on
      FreeRTOS + ThreadX as of writing.
- [ ] **Clock wraparound + extension correctness** on long-running
      deployments. The platform's `nros_platform_time_now_ms` must
      handle u32 wrap (49.7 days) and u64 extend.
- [ ] **Allocator behavior under memory pressure**. Boot-time alloc
      OK on most RTOSes; mid-run alloc only on `std` POSIX. Confirm
      your hot paths don't allocate.
- [ ] **Network packet loss recovery**. Drop 5% of packets in your
      lab and confirm the talker / listener recovers.
- [ ] **Critical-section regions** are short. The platform's
      `nros_platform_critical_section_*` ABI is the IRQ-disable
      surface; long critical sections starve other ISRs.

## 3. RMW backend certification

- [ ] **Backend version pinned** to a tested tuple. Zenoh-pico
      1.7.2 (matches `rmw_zenoh_cpp`). Cyclone DDS 0.10.5 (matches
      `ros-humble-cyclonedds`). dust-DDS at the workspace pin.
      XRCE-DDS Micro-Client at its workspace pin.
- [ ] **All required QoS policies supported** by your backend. The
      [Choosing an RMW Backend](../user-guide/rmw-backends.md)
      capability matrix lists per-backend coverage (Zenoh: 4/7;
      XRCE: 4/7; DDS: 7/7; Cyclone: 7/7).
- [ ] **Discovery stability** over your network topology.
      Zenoh-pico in client mode needs zenohd reachable; loss of
      router = lost routing but local node lives. XRCE needs
      Agent uptime. DDS / Cyclone discover via multicast SPDP.
- [ ] **Bridge stability** if multi-backend. Confirm no memory
      bloat over 72 h with two registered RMWs running.
- [ ] **Cyclone DDS limitations checked.** Services + actions
      return `NROS_RMW_RET_UNSUPPORTED`; status events not wired.
      If your design needs either, use Zenoh or dust-DDS.

## 4. Safety + formal verification

- [ ] **`just verify-kani`** clean against your build. 160 bounded
      harnesses; non-trivial coverage of CDR + scheduling + RMW
      glue.
- [ ] **`just verify-verus`** clean. 102 deductive proofs.
- [ ] **CRC32 attached** if using `safety-e2e` feature. The
      37-byte attachment is transparent to stock ROS 2 (ignored
      gracefully) and detected by other nano-ros nodes.
- [ ] **Timeout bounds on every blocking call.** `spin_once`
      timeout, `Promise::wait_for(timeout)`, `recv_timeout`. No
      `WAIT_FOREVER`.
- [ ] **Parameter store capacity** ≥ declared parameter count
      ([`param-services`] feature gate).
- [ ] **Stack overflow detection** enabled by your platform
      (FreeRTOS `configCHECK_FOR_STACK_OVERFLOW`, Zephyr stack
      sentinels, NuttX `CONFIG_DEBUG_STACK`).

## 5. Interop testing

- [ ] **Publish from nano-ros, subscribe with stock ROS 2.**
      `RMW_IMPLEMENTATION=rmw_zenoh_cpp` for Zenoh,
      `rmw_cyclonedds_cpp` for Cyclone, `rmw_fastrtps_cpp` for DDS
      (interop tier).
- [ ] **Message type compatibility** for any custom `.msg` you've
      added. Round-trip a sample message through ROS 2's
      `rosbag2` to confirm wire-level parity.
- [ ] **QoS profile matching.** Mismatched reliability /
      durability / history kill discovery silently on DDS / RTPS.
- [ ] **Lifecycle callbacks** fire on node startup / shutdown if
      you've opted into `lifecycle-services`.
- [ ] **Cross-RTOS interop**: if your fleet mixes RTOSes (e.g.
      Zephyr sensor + FreeRTOS actuator + POSIX coordinator),
      confirm all three sides see each other.

## 6. Failure recovery

- [ ] **Agent / router restart**: kill `zenohd` (Zenoh) or
      `MicroXRCEAgent` (XRCE) mid-run. Confirm reconnection. For
      DDS / Cyclone this is N/A (no central process).
- [ ] **Network partition → reconnection.** Block the talker's
      egress with `iptables` for 30 s, then unblock. Verify the
      listener resumes within your design SLA.
- [ ] **Heap exhaustion** path: graceful degradation OR clean
      crash + restart? If hosted-RTOS + watchdog, restart is
      usually correct. If bare-metal, you probably have no
      restart story — confirm your design assumes this.
- [ ] **Stack overflow detection** triggers a panic / fault
      rather than silent corruption.
- [ ] **Power loss mid-write** (if persisting state). Not
      nano-ros's concern, but mention it in your design review.

## 7. Operational concerns

- [ ] **Bootloader + OTA strategy.** Out of scope for nano-ros but
      mandatory for fleet deployments — name it explicitly in
      your project plan.
- [ ] **Log / diagnostics exfiltration.** `nros-log` provides the
      logging surface; pick a sink (UART, RTT, semihosting, or
      ROS 2 `/rosout` over the wire).
- [ ] **Time synchronization** (NTP, PTP, RTC). nano-ros doesn't
      ship a time-sync layer; your fleet design must.
- [ ] **Watchdog coverage**: the executor's `spin_period` reports
      overruns, but it doesn't pet a hardware watchdog. Wire one
      manually.

## 8. License + governance

- [ ] **License**: MIT OR Apache 2.0 (dual). Both permissive, no
      GPL copyleft, OK for proprietary firmware. Confirm your
      legal team is comfortable.
- [ ] **Third-party dependencies**: zenoh-pico (Eclipse), Cyclone
      DDS (Eclipse), dust-DDS (Apache-2), Micro-XRCE-DDS-Client
      (Apache-2). All vendored as submodules; license files in
      each `third-party/*/LICENSE`.
- [ ] **Patent grant**: Apache 2.0 carries an explicit patent
      grant; MIT does not. Most adopters rely on the Apache half.
- [ ] **Support model**: nano-ros has **no commercial support
      entity** as of writing. Plan accordingly — either staff
      in-house expertise or contract a consultancy.
- [ ] **Roadmap visibility**: track `docs/roadmap/` in the
      upstream repo. Phases are numbered and dated.

## Scoring rubric

For each section above, count `[x]` boxes as your readiness score.
Suggested gates:

| Score per section | Status |
|---|---|
| 8/8 | Production-ready for that axis |
| 5–7/8 | Pilot deployment OK; close gaps before scale |
| 3–4/8 | Lab / prototype only |
| < 3/8 | Block on these items first |

Sum across all 8 sections gives your overall readiness. Below 40/64
you have foundational work to do; above 56/64 you're at production
quality on every axis where nano-ros can be validated.

## See also

- [Real-Time Analysis](./realtime-analysis.md) — RT scheduling
  background + response-time formulas.
- [Formal Verification](./verification.md) — Kani + Verus harnesses.
- [Safety Protocol](./safety.md) — E2E CRC + sequence tracking.
- [Choosing an RMW Backend](../user-guide/rmw-backends.md) — backend
  capability matrix.
- [Supported Boards](../reference/supported-boards.md) — per-board
  status + caveats.
