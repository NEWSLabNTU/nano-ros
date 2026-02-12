# Autoware → nano-ros Porting Candidates

> Source: `~/repos/autoware/1.5.0-ws/src/` (Autoware 1.5.0)
> Date: 2026-02-07
> Related: [autoware-safety-island-analysis.md](autoware-safety-island-analysis.md)

## Evaluation Criteria

Each package is scored on two axes:

- **Portability** (how feasible is the rewrite): LOC count, external dependencies, algorithmic
  complexity, dynamic allocation usage, TF2/Eigen dependency
- **Contribution Value** (how much safety/operational value the port delivers): safety criticality,
  failure mode severity, how central it is in the Autoware pipeline

Scoring: 1-5 scale. **Priority = Portability x Value** (max 25).

---

## Tier 1: High Priority (Score >= 16)

### 1. autoware_mrm_emergency_stop_operator

| Metric | Value |
|--------|-------|
| Location | `universe/autoware_universe/system/` |
| LOC | 247 |
| External deps | None (pure ROS 2 messages) |
| ROS 2 patterns | Pub/Sub + Service server |
| Dynamic alloc | None in core logic |
| Portability | **5/5** |
| Contribution | **5/5** |
| **Priority** | **25** |

**What it does:** Receives emergency stop requests via service, publishes maximum-brake
`autoware_control_msgs/Control` commands (acceleration = -3.0 m/s^2, zero steering rate).
This is the last line of defense before vehicle hardware.

**Why port:** Running on an independent MCU means emergency braking works even if the main
compute crashes, the Linux kernel panics, or the ROS 2 daemon hangs. The algorithm is trivial
(publish a constant command), making it a perfect first port.

**nano-ros readiness:**
- Service server: supported
- Control message: generate via `cargo nano-ros generate`
- Timer (10 Hz): supported
- Lifecycle node: supported (for clean startup/shutdown)

**Interfaces to implement:**
```
Service: ~/input/mrm/emergency_stop/operate  (tier4_system_msgs/srv/OperateMrm)
Pub:     ~/output/mrm/emergency_stop/status  (tier4_system_msgs/msg/MrmBehaviorStatus)
Pub:     /control/command/emergency_cmd       (autoware_control_msgs/msg/Control)
```

**Estimated effort:** 3-5 days

---

### 2. autoware_mrm_comfortable_stop_operator

| Metric | Value |
|--------|-------|
| Location | `universe/autoware_universe/system/` |
| LOC | 217 |
| External deps | None |
| ROS 2 patterns | Pub/Sub + Service server |
| Dynamic alloc | None in core |
| Portability | **5/5** |
| Contribution | **4/5** |
| **Priority** | **20** |

**What it does:** Gentle deceleration variant (-1.0 m/s^2 typical). Activated before emergency
stop when the system detects degraded autonomy. Publishes ramping-down velocity commands.

**Why port:** Provides graceful degradation on the safety island. If the main stack recovers,
the comfortable stop can be canceled. If not, the emergency stop operator takes over.

**Interfaces:**
```
Service: ~/input/mrm/comfortable_stop/operate  (tier4_system_msgs/srv/OperateMrm)
Pub:     ~/output/mrm/comfortable_stop/status   (tier4_system_msgs/msg/MrmBehaviorStatus)
Pub:     /control/command/emergency_cmd           (autoware_control_msgs/msg/Control)
Sub:     /localization/kinematic_state             (nav_msgs/msg/Odometry) [for current velocity]
```

**Estimated effort:** 3-5 days

---

### 3. autoware_stop_filter

| Metric | Value |
|--------|-------|
| Location | `core/autoware_core/localization/` |
| LOC | 482 (core logic: ~50) |
| External deps | None |
| ROS 2 patterns | Pub/Sub only |
| Dynamic alloc | None |
| Portability | **5/5** |
| Contribution | **4/5** |
| **Priority** | **20** |

**What it does:** Determines whether the vehicle is stopped by checking velocity thresholds:
`is_stopped = (|vx| < vx_thresh) AND (|wz| < wz_thresh)`. Prevents false motion reports
from noisy sensors, which is critical for safe state transitions (park, engage, disengage).

**Why port:** A corrupted "vehicle is stopped" signal on the main stack could cause the system
to engage autonomous mode while the vehicle is moving, or fail to apply the parking brake.
Running this on the safety island provides an independent motion oracle.

**Core algorithm (extractable):**
```rust
fn is_stopped(vx: f32, wz: f32, vx_thresh: f32, wz_thresh: f32) -> bool {
    vx.abs() < vx_thresh && wz.abs() < wz_thresh
}
```

**Interfaces:**
```
Sub: /localization/kinematic_state  (nav_msgs/msg/Odometry)
Pub: /localization/kinematic_state  (nav_msgs/msg/Odometry) [with corrected twist when stopped]
```

**Estimated effort:** 1-2 days

---

### 4. autoware_vehicle_velocity_converter

| Metric | Value |
|--------|-------|
| Location | `core/autoware_core/sensing/` |
| LOC | 323 (core: ~60) |
| External deps | None |
| ROS 2 patterns | Pub/Sub only |
| Dynamic alloc | None |
| Portability | **5/5** |
| Contribution | **4/5** |
| **Priority** | **20** |

**What it does:** Converts vehicle CAN velocity reports (`VelocityReport`) to standard
`TwistWithCovarianceStamped`. Stateless format translation with covariance matrix
assignment (36 float values).

**Why port:** Running directly on the MCU that reads CAN means velocity data never touches
the main Linux stack. The safety island gets ground-truth velocity from hardware, which
feeds the stop filter and MRM operators.

**Interfaces:**
```
Sub: /vehicle/status/velocity_status        (autoware_vehicle_msgs/msg/VelocityReport)
Pub: /vehicle/status/twist_with_covariance  (geometry_msgs/msg/TwistWithCovarianceStamped)
```

**Estimated effort:** 1-2 days

---

### 5. autoware_shift_decider

| Metric | Value |
|--------|-------|
| Location | `universe/autoware_universe/control/` |
| LOC | 163 |
| External deps | generate_parameter_library (parameters only) |
| ROS 2 patterns | Pub/Sub + Timer |
| Dynamic alloc | None |
| Portability | **5/5** |
| Contribution | **4/5** |
| **Priority** | **20** |

**What it does:** Simple state machine that decides vehicle gear (DRIVE/REVERSE/PARK/NEUTRAL)
based on the current control command velocity sign. Prevents gear-hunting and ensures safe
transitions.

**Why port:** Gear commands go directly to the vehicle CAN bus. A stuck gear decision on
the main stack could mean the vehicle can't shift to PARK during emergency stop.

**Interfaces:**
```
Sub: /control/command/control_cmd  (autoware_control_msgs/msg/Control)
Pub: /control/command/gear_cmd     (autoware_vehicle_msgs/msg/GearCommand)
```

**Estimated effort:** 1-2 days

---

### 6. autoware_vehicle_cmd_gate

| Metric | Value |
|--------|-------|
| Location | `universe/autoware_universe/control/` |
| LOC | 3,215 |
| External deps | None (pure ROS 2) |
| ROS 2 patterns | Pub/Sub (10+ subs) + Timer + Services |
| Dynamic alloc | Minimal (string formatting for diagnostics) |
| Portability | **3/5** |
| Contribution | **5/5** |
| **Priority** | **15** → bumped to **18** (critical safety role) |

**What it does:** The **final safety filter** before actuators. Arbitrates between autonomous,
remote, and emergency control sources. Enforces rate limits on steering, acceleration, and
jerk. Monitors heartbeat from the autonomy stack and triggers emergency stop on timeout.

**Why port:** This is the single most safety-critical software gate in the entire Autoware
stack. If it runs on the safety island, no software failure on the main computer can send
unchecked commands to the vehicle.

**Key algorithms:**
- Command rate limiting (velocity, acceleration, jerk, steering angle rate)
- Heartbeat timeout detection
- Source priority arbitration (emergency > remote > autonomous)
- Parameter-driven safety limits

**Complexity note:** 3,215 LOC is significant but the core algorithms are straightforward
arithmetic (clamping, rate limiting, timeout checks). No matrix math, no optimization.
The bulk is interface wiring (10+ subscribers, multiple command sources).

**Interfaces (critical subset):**
```
Sub: /control/command/control_cmd        (autoware_control_msgs/msg/Control) [from autonomy]
Sub: /control/command/emergency_cmd      (autoware_control_msgs/msg/Control) [from MRM]
Sub: /system/emergency/is_emergency      (autoware_vehicle_msgs/msg/ControlModeReport)
Sub: /system/operation_mode/state        (autoware_system_msgs/msg/OperationModeState)
Pub: /control/command/control_cmd_gate   (autoware_control_msgs/msg/Control) [to actuators]
Pub: /diagnostics                        (diagnostic_msgs/msg/DiagnosticArray)
Srv: ~/engage                            (tier4_external_api_msgs/srv/Engage)
```

**Estimated effort:** 10-15 days (phased: core gate logic first, then full interface wiring)

---

## Tier 2: Medium Priority (Score 10-15)

### 7. autoware_mrm_handler

| Metric | Value |
|--------|-------|
| Location | `universe/autoware_universe/system/` |
| LOC | 808 |
| External deps | None |
| ROS 2 patterns | Pub/Sub + Timer + Service clients |
| Dynamic alloc | Minimal |
| Portability | **4/5** |
| Contribution | **5/5** |
| **Priority** | **20** → adjusted to **15** (depends on items 1-2 being ported first) |

**What it does:** The MRM orchestrator. Receives system health from the diagnostic graph
aggregator, decides which MRM behavior to activate (emergency stop, comfortable stop,
pull over), and calls the corresponding operator service.

**Why port:** Running the MRM decision logic on the safety island means the island can
autonomously decide to brake even if the main stack is completely unresponsive. However,
this only provides value after the MRM operators (items 1-2) are ported.

**Key state machine:**
```
NORMAL → COMFORTABLE_STOP → EMERGENCY_STOP
  ↑           ↓                    ↓
  └── RECOVERY ←──── (if velocity == 0 for timeout)
```

**Interfaces:**
```
Sub: ~/input/operation_mode_availability  (tier4_system_msgs/msg/OperationModeAvailability)
Sub: /localization/kinematic_state        (nav_msgs/msg/Odometry)
Pub: ~/output/mrm/state                  (autoware_adapi_v1_msgs/msg/MrmState)
Pub: ~/output/hazard                     (autoware_vehicle_msgs/msg/HazardLightsCommand)
Pub: ~/output/gear                       (autoware_vehicle_msgs/msg/GearCommand)
Cli: ~/output/mrm/emergency_stop/operate (tier4_system_msgs/srv/OperateMrm)
Cli: ~/output/mrm/comfortable_stop/operate (tier4_system_msgs/srv/OperateMrm)
```

**Estimated effort:** 5-8 days

---

### 8. autoware_twist2accel

| Metric | Value |
|--------|-------|
| Location | `core/autoware_core/localization/` |
| LOC | 187 (core: ~100) |
| External deps | autoware_signal_processing (lowpass filter) |
| ROS 2 patterns | Pub/Sub only |
| Dynamic alloc | 6 shared_ptr (refactorable) |
| Portability | **4/5** |
| Contribution | **3/5** |
| **Priority** | **12** |

**What it does:** Numerical differentiation of twist (velocity) to produce acceleration,
with 1st-order lowpass filtering to suppress noise. The PID longitudinal controller
uses this acceleration estimate for feedforward compensation.

**Core algorithm:**
```rust
let accel = (twist_now - twist_prev) / dt;
let filtered = prev + gain * (accel - prev);  // lowpass
```

**Why port:** On the safety island, this provides acceleration estimation from velocity
without depending on the main stack's processing pipeline. Useful for the MRM comfortable
stop operator to know current deceleration rate.

**Estimated effort:** 2-3 days

---

### 9. autoware_control_validator

| Metric | Value |
|--------|-------|
| Location | `universe/autoware_universe/control/` |
| LOC | 1,499 |
| External deps | autoware_signal_processing |
| ROS 2 patterns | Pub/Sub + Timer + Diagnostics |
| Dynamic alloc | Minimal |
| Portability | **3/5** |
| Contribution | **4/5** |
| **Priority** | **12** |

**What it does:** Validates control commands against safety constraints:
- Lateral deviation from trajectory exceeds threshold → WARN/ERROR
- Velocity error exceeds threshold → WARN/ERROR
- Publishes diagnostic status for the diagnostic graph aggregator

**Why port:** An independent control validator on the safety island can detect when the
main stack's controller is sending dangerous commands (e.g., steering off-trajectory)
and trigger MRM before the main stack's own validator detects the issue.

**Estimated effort:** 5-7 days

---

### 10. autoware_operation_mode_transition_manager

| Metric | Value |
|--------|-------|
| Location | `universe/autoware_universe/system/` |
| LOC | 1,457 |
| External deps | None |
| ROS 2 patterns | Pub/Sub + Timer + Services |
| Dynamic alloc | Minimal |
| Portability | **3/5** |
| Contribution | **4/5** |
| **Priority** | **12** |

**What it does:** Manages transitions between MANUAL, AUTONOMOUS, and REMOTE operation
modes. Validates preconditions before allowing mode changes (e.g., vehicle must be stopped
to engage autonomous mode, all system checks must pass).

**Why port:** If the main stack falsely reports "ready for autonomous mode," the safety
island's independent mode manager can refuse the transition. This prevents engaging
autonomy with degraded sensors or failed diagnostics.

**Estimated effort:** 5-8 days

---

### 11. autoware_hazard_status_converter

| Metric | Value |
|--------|-------|
| Location | `universe/autoware_universe/system/` |
| LOC | 184 |
| External deps | None |
| ROS 2 patterns | Pub/Sub only |
| Dynamic alloc | None |
| Portability | **5/5** |
| Contribution | **2/5** |
| **Priority** | **10** |

**What it does:** Converts diagnostic graph status → HazardStatus messages for external
monitoring systems (fleet management, remote operator console).

**Why port:** Low effort, provides the safety island with a standard hazard status
publisher that external systems can monitor independently.

**Estimated effort:** 1-2 days

---

## Tier 3: Lower Priority (Score < 10)

### 12. autoware_simple_pure_pursuit

| Metric | Value |
|--------|-------|
| LOC | 378 |
| Portability | **5/5** |
| Contribution | **2/5** |
| **Priority** | **10** |

Simple geometric path follower. Could serve as a minimal fallback controller on the safety
island (follow a pre-recorded safe path to pull over). Low value unless combined with a
pre-planned emergency trajectory.

### 13. autoware_gyro_odometer

| Metric | Value |
|--------|-------|
| LOC | 673 (core: ~200) |
| Portability | **3/5** (needs deque→circular buffer, TF2 removal) |
| Contribution | **3/5** |
| **Priority** | **9** |

Sensor fusion of gyroscope + wheel odometry. Provides angular velocity estimates.
Useful for the safety island's own dead-reckoning but requires refactoring dynamic
allocations and removing TF2 dependency.

### 14. autoware_gnss_poser

| Metric | Value |
|--------|-------|
| LOC | 1,182 (core projection: ~400) |
| Portability | **2/5** (geographiclib dependency) |
| Contribution | **3/5** |
| **Priority** | **6** |

WGS84→UTM coordinate projection. Would give the safety island GPS-based position
without depending on the main stack's NDT localization. Requires porting geographiclib
(or a simplified UTM projection).

### 15. autoware_pid_longitudinal_controller

| Metric | Value |
|--------|-------|
| LOC | 3,453 |
| Portability | **2/5** (Eigen dependency, complex state machine) |
| Contribution | **4/5** |
| **Priority** | **8** |

Full PID controller with slope compensation, smooth stop, delay compensation. Too complex
for a first port but could eventually replace the simple "constant deceleration" in the
MRM operators with a proper velocity-tracking controller on the safety island.

---

## nano-ros Capability Gaps

| Required Capability | nano-ros Status | Gap | Mitigation |
|---|---|---|---|
| autoware_control_msgs | Not generated | Must generate | `cargo nano-ros generate` from Autoware .msg files |
| tier4_system_msgs | Not generated | Must generate | Same approach |
| autoware_vehicle_msgs | Not generated | Must generate | Same approach |
| nav_msgs/Odometry | Not generated | Must generate | Same approach |
| diagnostic_msgs | Not generated | Must generate | Same approach |
| geometry_msgs | Not generated | Must generate | Same approach |
| TF2 transforms | Not supported | No tree manager | Inline static transforms for known frames |
| Eigen linear algebra | Not included | No matrix types | Use `nalgebra` (no_std) or `micromath` |
| diagnostic_updater | Not implemented | No aggregator integration | Custom diagnostic publisher |
| Heartbeat/watchdog | Not built-in | Must implement | Timer-based watchdog (trivial with nano-ros timers) |

**Critical path:** Message generation. All Tier 1 candidates need `autoware_control_msgs`,
`tier4_system_msgs`, and `autoware_vehicle_msgs` generated for nano-ros.

### Formal Verification Readiness

Ported components benefit from nano-ros's verification pipeline:

| Tool | What it proves | Relevance to Autoware ports |
|------|---------------|---------------------------|
| **Kani** (82 harnesses) | Panic-freedom, CDR correctness, bounded resources | Serialization of Autoware messages is safe; parameter handling is correct |
| **Verus** (10 proofs) | Timer drift-free scheduling, trigger gating, spin_once consistency | MRM operators and watchdog timers have provably correct scheduling |
| **DWT cycle counters** | Measured WCET per operation | Validates that safety island meets 10/30/100 Hz deadlines on target hardware |
| **Static stack analysis** | Per-function stack frames | Ensures ported components fit within MCU stack limits (e.g., 8 KB on STM32F4) |

Any ported Autoware component running on nano-ros automatically inherits these guarantees for the underlying executor, serialization, and timer infrastructure.

---

## Recommended Porting Roadmap

### Phase A: Foundation (Weeks 1-2)

Generate required message types and build a minimal safety island prototype.

1. **Generate messages** for autoware_control_msgs, tier4_system_msgs, autoware_vehicle_msgs,
   nav_msgs, geometry_msgs
2. **Port autoware_stop_filter** (1-2 days) — validates the message generation pipeline
3. **Port autoware_vehicle_velocity_converter** (1-2 days) — validates CAN→ROS bridge
4. **Port autoware_shift_decider** (1-2 days) — validates state machine pattern

### Phase B: Emergency Response (Weeks 3-4)

Build the core MRM chain on the safety island.

5. **Port autoware_mrm_emergency_stop_operator** (3-5 days) — service-based emergency braking
6. **Port autoware_mrm_comfortable_stop_operator** (3-5 days) — gentle deceleration
7. **Port autoware_mrm_handler** (5-8 days) — orchestrates the above two
8. **Implement watchdog timer** — monitors main stack heartbeat, triggers MRM on timeout

### Phase C: Safety Gate (Weeks 5-7)

Move the final safety filter to the island.

9. **Port autoware_vehicle_cmd_gate** (10-15 days, phased):
   - Week 5: Core rate limiting + command clamping
   - Week 6: Source arbitration (emergency > remote > auto)
   - Week 7: Heartbeat monitoring + diagnostic publishing

### Phase D: Validation Layer (Weeks 8-9)

Add independent validation on the safety island.

10. **Port autoware_control_validator** (5-7 days) — trajectory deviation checker
11. **Port autoware_operation_mode_transition_manager** (5-8 days) — mode gate
12. **Port autoware_twist2accel** (2-3 days) — acceleration estimation for MRM

---

## Architecture: Safety Island Integration

```
┌──────────────────────────────────────────────────────────┐
│                    Main Compute (x86/ARM64)               │
│  ┌─────────┐  ┌──────────┐  ┌─────────┐  ┌───────────┐ │
│  │ Sensing  │→│ Planning  │→│ Control  │→│ Main Gate  │ │
│  │ (LiDAR,  │  │ (Behavior │  │ (MPC +   │  │ (original) │ │
│  │  Camera)  │  │  Planner) │  │  PID)    │  │            │ │
│  └─────────┘  └──────────┘  └─────────┘  └─────┬─────┘ │
│                                                   │       │
│                        zenoh (ROS 2 domain)       │       │
└────────────────────────────────────────────────────┼───────┘
                                                     │
                    ┌────── zenoh-pico ──────────────┤
                    │                                │
┌───────────────────┼────────────────────────────────┼──────┐
│  Safety Island    │ (STM32F4 / Cortex-M4 + RTIC)  │      │
│                   ▼                                ▼      │
│  ┌──────────────────┐    ┌──────────────────────┐        │
│  │  Vehicle Cmd Gate │◄──│ MRM Handler           │        │
│  │  (rate limiting,  │    │ (state machine:       │        │
│  │   source arbiter) │    │  NORMAL→COMF→EMERG)  │        │
│  └────────┬─────────┘    └──────┬───────────────┘        │
│           │                      │                        │
│           ▼                      ├──► Emergency Stop Op   │
│  ┌──────────────────┐           ├──► Comfortable Stop Op │
│  │ Control Validator │           │                        │
│  │ (deviation check) │    ┌─────┴──────────┐             │
│  └──────────────────┘    │ Watchdog Timer  │             │
│                           │ (heartbeat mon) │             │
│  ┌──────────────────┐    └────────────────┘             │
│  │ Stop Filter       │                                    │
│  │ Velocity Converter│    ┌────────────────┐             │
│  │ Shift Decider     │    │ Mode Transition │             │
│  └────────┬─────────┘    │ Manager         │             │
│           │               └────────────────┘             │
│           ▼                                               │
│       CAN Bus ──────────────────────────► Vehicle HW     │
└──────────────────────────────────────────────────────────┘
```

**Data flow on the safety island:**

1. Vehicle velocity arrives via CAN → `velocity_converter` → `stop_filter`
2. Main stack control commands arrive via zenoh → `vehicle_cmd_gate` (rate limits, clamps)
3. `watchdog_timer` monitors main stack heartbeat (zenoh topic)
4. On heartbeat timeout → `mrm_handler` activates → `emergency_stop_operator` publishes brake
5. `vehicle_cmd_gate` prioritizes emergency commands over main stack commands
6. Final command goes to CAN bus → vehicle actuators

**Key property:** The safety island can bring the vehicle to a safe stop with **zero
dependency** on the main compute. It only needs: CAN bus (velocity in, commands out)
and zenoh-pico (to receive commands from main stack when healthy).
