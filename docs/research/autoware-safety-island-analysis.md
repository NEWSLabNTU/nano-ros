# Autoware 1.5.0 Safety Island Analysis for nano-ros

> Source: `~/repos/autoware/1.5.0-ws/src/` (Autoware 1.5.0)
> Date: 2026-02-07

## 1. Overview

A **safety island** is an independent compute unit (e.g., STM32F4 running RTIC) that
enforces safety invariants even when the main Autoware stack (Linux/ROS 2 on x86/ARM64) fails.
nano-ros bridges the island to the ROS 2 network via zenoh; RTIC provides hard real-time
guarantees with compile-time deadlock freedom and WCET analyzability.

This document catalogs Autoware 1.5.0 components suitable for porting to nano-ros safety
islands, with exact ROS 2 interface specifications extracted from source code.

## 2. Autoware Architecture At a Glance

```
Sensors → Perception → Planning → Control → Vehicle Interface → Actuators
              ↑                       ↑              ↑
          Localization            Validators     Safety Gate
              ↑                       ↑              ↑
          System Monitor ──→ Diagnostic Graph ──→ MRM Handler
```

**300+ packages** organized as:

| Layer         | Location               | Examples                                   |
|---------------|------------------------|--------------------------------------------|
| core          | `core/autoware_core/`  | EKF, gyro odometer, stop filter            |
| universe      | `universe/autoware_universe/` | MPC, PID, AEB, MRM, diagnostics     |
| sensor        | `sensor_component/`    | LiDAR drivers, CAN, serial                 |
| middleware    | `middleware/`          | Message definitions, utilities              |
| launcher      | `launcher/`            | Launch files, vehicle configs               |

## 3. Critical Data Flow: Control Pipeline

```
                    Trajectory (10 Hz)
                         │
              ┌──────────┴──────────┐
              ▼                     ▼
   MPC Lateral Controller    PID Longitudinal Controller
   (steering angle)          (acceleration + state machine)
              │                     │
              └──────────┬──────────┘
                         ▼
              autoware_control_msgs/Control
                         │
                         ▼
               Vehicle Command Gate  ◄── Emergency heartbeat
               (safety filter)       ◄── MRM state
                         │
                         ▼
              Raw Vehicle Cmd Converter
              (accel→throttle/brake maps)
                         │
                         ▼
                  CAN / Vehicle HW
```

All nodes in this pipeline run at **10 Hz**.

## 4. Safety-Critical Message Types

### 4.1 Control Commands

**autoware_control_msgs/msg/Control** — Primary control message:
```
builtin_interfaces/Time stamp
builtin_interfaces/Time control_time
Lateral lateral
Longitudinal longitudinal
```

**Lateral**:
```
float32 steering_tire_angle          # rad
float32 steering_tire_rotation_rate  # rad/s
bool is_defined_steering_tire_rotation_rate
```

**Longitudinal**:
```
float32 velocity       # m/s
float32 acceleration   # m/s²
float32 jerk           # m/s³
bool is_defined_acceleration
bool is_defined_jerk
```

### 4.2 Vehicle State

**autoware_vehicle_msgs/msg/SteeringReport**: `{ stamp, float32 steering_tire_angle }`

**autoware_vehicle_msgs/msg/VelocityReport**: `{ header, float32 longitudinal_velocity, float32 lateral_velocity, float32 heading_rate }`

**autoware_vehicle_msgs/msg/GearCommand**: `{ stamp, uint8 command }` — NONE(0) NEUTRAL(1) DRIVE(2) REVERSE(20) PARK(22)

**autoware_vehicle_msgs/msg/ControlModeReport**: `{ stamp, uint8 mode }` — AUTONOMOUS(1) MANUAL(4) DISENGAGED(5)

**autoware_vehicle_msgs/msg/HazardLightsCommand**: `{ stamp, uint8 command }` — DISABLE(1) ENABLE(2)

**autoware_vehicle_msgs/msg/TurnIndicatorsCommand**: `{ stamp, uint8 command }` — DISABLE(1) LEFT(2) RIGHT(3)

### 4.3 Operation Mode & MRM

**autoware_adapi_v1_msgs/msg/OperationModeState**:
```
uint8 mode                         # STOP(1) AUTONOMOUS(2) LOCAL(3) REMOTE(4)
bool is_autoware_control_enabled
bool is_in_transition
bool is_stop_mode_available
bool is_autonomous_mode_available
bool is_local_mode_available
bool is_remote_mode_available
```

**autoware_adapi_v1_msgs/msg/MrmState**:
```
uint16 state     # NORMAL(1) MRM_OPERATING(2) MRM_SUCCEEDED(3) MRM_FAILED(4)
uint16 behavior  # NONE(1) EMERGENCY_STOP(2) COMFORTABLE_STOP(3) PULL_OVER(4)
```

**autoware_adapi_v1_msgs/msg/Heartbeat**: `{ stamp, uint16 seq }`

**autoware_adapi_v1_msgs/msg/ManualOperatorHeartbeat**: `{ stamp, bool ready }`

**tier4_system_msgs/msg/OperationModeAvailability**:
```
builtin_interfaces/Time stamp
bool stop
bool autonomous
bool local
bool remote
bool emergency_stop
bool comfortable_stop
bool pull_over
```

**tier4_system_msgs/msg/MrmBehaviorStatus**:
```
# NOT_AVAILABLE(0) AVAILABLE(1) OPERATING(2)
uint8 state
```

**tier4_system_msgs/srv/OperateMrm**: `{ Request: bool operate } → { Response: success }`

### 4.4 Trajectory (Planning → Control)

**autoware_planning_msgs/msg/Trajectory**: `{ header, TrajectoryPoint[] points }`

**TrajectoryPoint**:
```
builtin_interfaces/Duration time_from_start
geometry_msgs/Pose pose
float32 longitudinal_velocity_mps
float32 lateral_velocity_mps
float32 acceleration_mps2
float32 heading_rate_rps
float32 front_wheel_angle_rad
```

## 5. Component Specifications

### 5.1 Vehicle Command Gate (Safety Filter)

**Path**: `universe/autoware_universe/control/autoware_vehicle_cmd_gate/`
**Frequency**: 10 Hz

The **last line of defense** before actuators. Clamps all commands to safe ranges.

#### Interface

| Direction | Topic | Type |
|-----------|-------|------|
| Sub | `input/auto/control_cmd` | Control |
| Sub | `input/external/control_cmd` | Control |
| Sub | `input/emergency/control_cmd` | Control |
| Sub | `input/external_emergency_stop_heartbeat` | ManualOperatorHeartbeat |
| Sub | `input/operation_mode` | OperationModeState |
| Sub | `input/mrm_state` | MrmState |
| Sub | `/localization/kinematic_state` | nav_msgs/Odometry |
| Sub | `input/steering` | SteeringReport |
| Pub | `output/control_cmd` | Control |
| Pub | `output/gear_cmd` | GearCommand |
| Pub | `output/turn_indicators_cmd` | TurnIndicatorsCommand |
| Pub | `output/hazard_lights_cmd` | HazardLightsCommand |
| Pub | `output/vehicle_cmd_emergency` | VehicleEmergencyStamped |

#### Safety Limits (interpolated by velocity)

Reference speed points: `[0.1, 0.3, 20.0, 30.0]` m/s

| Limit | Nominal Values | Unit |
|-------|---------------|------|
| Velocity | 25.0 | m/s |
| Longitudinal acceleration | [5.0, 5.0, 5.0, 4.0] | m/s² |
| Longitudinal jerk | [80.0, 5.0, 5.0, 4.0] | m/s³ |
| Steering angle | [1.0, 1.0, 1.0, 0.8] | rad |
| Steering rate | [1.0, 1.0, 1.0, 0.8] | rad/s |
| Lateral acceleration | [5.0, 5.0, 5.0, 4.0] | m/s² |
| Lateral jerk | [7.0, 7.0, 7.0, 6.0] | m/s³ |

Special accelerations: `stop_hold: -1.5`, `emergency: -2.4` m/s².

**Gate logic**: Three command sources (AUTO, EXTERNAL, EMERGENCY) selected by
engagement state; EMERGENCY always wins.

### 5.2 MRM Handler (Emergency Coordinator)

**Path**: `universe/autoware_universe/system/autoware_mrm_handler/`
**Frequency**: 10 Hz

Orchestrates emergency response. Monitors component health via diagnostic graph
and triggers appropriate MRM behavior.

#### Interface

| Direction | Topic | Type |
|-----------|-------|------|
| Sub | `~/input/operation_mode_availability` | OperationModeAvailability |
| Sub | `~/input/odometry` | nav_msgs/Odometry |
| Sub | `~/input/control_mode` | ControlModeReport |
| Sub | `~/input/mrm/comfortable_stop/status` | MrmBehaviorStatus |
| Sub | `~/input/mrm/emergency_stop/status` | MrmBehaviorStatus |
| Sub | `~/input/api/operation_mode/state` | OperationModeState |
| Pub | `~/output/mrm/state` | MrmState |
| Pub | `~/output/hazard` | HazardLightsCommand |
| Pub | `~/output/gear` | GearCommand |
| Srv | `~/output/mrm/comfortable_stop/operate` | OperateMrm |
| Srv | `~/output/mrm/emergency_stop/operate` | OperateMrm |

#### State Machine

```
NORMAL ──[emergency detected]──→ MRM_OPERATING
  ↑                                    │
  │                              ┌─────┴─────┐
  │                              ▼           ▼
  └──[recovered]──── MRM_SUCCEEDED    MRM_FAILED
```

**Emergency triggers**:
- Current operation mode not available (component health failure)
- `operation_mode_availability` watchdog timeout (0.5 s)
- Emergency holding active

**MRM behavior selection** (priority order):
1. If watchdog timeout → EMERGENCY_STOP
2. If comfortable_stop available → COMFORTABLE_STOP
3. Else → EMERGENCY_STOP

On any service call failure, immediately escalate to EMERGENCY_STOP.

### 5.3 MRM Emergency Stop Operator

**Path**: `universe/autoware_universe/system/autoware_mrm_emergency_stop_operator/`
**Frequency**: 30 Hz

#### Interface

| Direction | Topic/Service | Type |
|-----------|---------------|------|
| Srv | `~/input/mrm/emergency_stop/operate` | OperateMrm |
| Sub | `~/input/control/control_cmd` | Control |
| Pub | `~/output/mrm/emergency_stop/status` | MrmBehaviorStatus |
| Pub | `~/output/mrm/emergency_stop/control_cmd` | Control |

#### Algorithm

Jerk-limited deceleration ramp:
```
a(t+1) = max(a(t) + target_jerk × dt, target_acceleration)
v(t+1) = max(v(t) + a(t) × dt, 0.0)
```

Parameters: `target_acceleration: -2.5 m/s²`, `target_jerk: -1.5 m/s³`

### 5.4 MRM Comfortable Stop Operator

**Path**: `universe/autoware_universe/system/autoware_mrm_comfortable_stop_operator/`
**Frequency**: 10 Hz

#### Interface

| Direction | Topic/Service | Type |
|-----------|---------------|------|
| Srv | `~/input/mrm/comfortable_stop/operate` | OperateMrm |
| Pub | `~/output/mrm/comfortable_stop/status` | MrmBehaviorStatus |
| Pub | `~/output/velocity_limit` | VelocityLimit |

Sets velocity limit to 0.0 m/s with constraints:
`min_acceleration: -1.0 m/s²`, `max_jerk: 0.3 m/s³`, `min_jerk: -0.3 m/s³`

### 5.5 PID Longitudinal Controller

**Path**: `universe/autoware_universe/control/autoware_pid_longitudinal_controller/`
**Frequency**: 10 Hz

#### Interface

| Direction | Topic | Type |
|-----------|-------|------|
| Sub | (via trajectory_follower) | Trajectory, Odometry, SteeringReport, OperationModeState |
| Pub | (via trajectory_follower) | Longitudinal |
| Pub | `~/output/slope_angle` | Float32MultiArrayStamped |

#### State Machine

```
STOPPED ──→ DRIVE ──→ STOPPING ──→ STOPPED
   │          │                       ↑
   └──────────┴──→ EMERGENCY ─────────┘
```

| Transition | Condition |
|-----------|-----------|
| STOPPED → DRIVE | Target velocity > 0 |
| DRIVE → STOPPING | stop_dist < 0.5 m |
| STOPPING → STOPPED | velocity < 0.01 m/s for 0.1 s |
| Any → EMERGENCY | Overshoot > 1.5 m past stop |

#### PID Parameters

| Parameter | Value |
|-----------|-------|
| Kp | 1.0 |
| Ki | 0.1 |
| Kd | 0.0 |
| max_acc | 3.0 m/s² |
| min_acc | -5.0 m/s² |
| max_jerk | 2.0 m/s³ |
| min_jerk | -5.0 m/s³ |
| lpf_vel_error_gain | 0.9 |
| delay compensation | 0.17 s (2 steps) |

Slope compensation via trajectory pitch angle.

### 5.6 MPC Lateral Controller

**Path**: `universe/autoware_universe/control/autoware_mpc_lateral_controller/`
**Frequency**: 10 Hz

#### Interface

| Direction | Topic | Type |
|-----------|-------|------|
| Sub | (via trajectory_follower) | Trajectory, Odometry, SteeringReport, OperationModeState |
| Pub | (via trajectory_follower) | Lateral |
| Pub | `~/output/predicted_trajectory` | Trajectory |

#### MPC Specification

| Parameter | Value |
|-----------|-------|
| Prediction horizon | 50 steps × 0.1 s = **5.0 s** |
| State vector | [lat_error, yaw_error, lat_vel_error, yaw_rate_error] (4D) |
| Input | steering angle (1D) |
| Solver | OSQP (quadratic program) |
| Vehicle model | Bicycle kinematics with 1st-order steering lag (τ=0.27 s) |
| Input delay compensation | 0.24 s |
| Steering LPF cutoff | 3.0 Hz |
| Matrix sizes | Qex: 200×200, R: 50×50 |

**Computational cost**: Too heavy for Cortex-M. Suitable for cross-check only (simplified 2-step MPC
or pure pursuit fallback on safety island).

### 5.7 EKF Localizer

**Path**: `core/autoware_core/localization/autoware_ekf_localizer/`
**Frequency**: 50 Hz prediction, 50 Hz TF broadcast

#### Interface

| Direction | Topic | Type |
|-----------|-------|------|
| Sub | `in_pose_with_covariance` | PoseWithCovarianceStamped |
| Sub | `in_twist_with_covariance` | TwistWithCovarianceStamped |
| Pub | `ekf_odom` | nav_msgs/Odometry |
| Pub | `ekf_twist_with_covariance` | TwistWithCovarianceStamped |

#### State Vector (6D)

| Index | Variable | Description |
|-------|----------|-------------|
| 0 | X | Position X |
| 1 | Y | Position Y |
| 2 | YAW | Heading |
| 3 | YAWB | Yaw bias |
| 4 | VX | Longitudinal velocity |
| 5 | WZ | Yaw rate |

Plus 1D filters for Z, roll, pitch.

#### Diagnostic Thresholds

| Check | Warn | Error |
|-------|------|-------|
| Pose measurement gap | 50 cycles | 100 cycles |
| Twist measurement gap | 50 cycles | 100 cycles |
| Pose Mahalanobis gate | — | > 49.5 |
| Covariance ellipse (long) | ≥ 1.2 m | ≥ 1.5 m |
| Covariance ellipse (lateral) | ≥ 0.25 m | ≥ 0.3 m |

### 5.8 Gyro Odometer

**Path**: `core/autoware_core/localization/autoware_gyro_odometer/`
**Frequency**: 100 Hz subscription, 10 Hz diagnostics

#### Interface

| Direction | Topic | Type |
|-----------|-------|------|
| Sub | `vehicle/twist_with_covariance` | TwistWithCovarianceStamped |
| Sub | `imu` | sensor_msgs/Imu |
| Pub | `twist_with_covariance` | TwistWithCovarianceStamped |

Fuses wheel odometry linear velocity with IMU gyroscope angular velocity.
Clears angular velocity when stopped (vx < 0.01 m/s AND wz < 0.01 rad/s).
Message timeout: 0.2 s.

### 5.9 Control Validator

**Path**: `universe/autoware_universe/control/autoware_control_validator/`

#### Validations

| Check | Threshold | Level |
|-------|-----------|-------|
| Control latency | 0.01 s nominal | ERROR |
| Trajectory deviation | 1.0 m max | ERROR |
| Lateral jerk | 10.0 m/s³ | ERROR |
| Acceleration error | offset=0.8 m/s², scale=20% | ERROR |
| Rolling back | 0.5 m/s opposite direction | ERROR |
| Overspeed | 20% + 2.0 m/s over target | ERROR |
| Overrun stop point | 0.8 m past stop line | ERROR |
| Yaw deviation | 0.5 rad warn, 1.0 rad error | WARN/ERROR |

### 5.10 Localization Error Monitor

**Path**: `universe/autoware_universe/localization/autoware_localization_error_monitor/`

Monitors EKF output covariance. Computes 3σ ellipse from pose covariance matrix.

| Threshold | Warn | Error |
|-----------|------|-------|
| Ellipse long axis | ≥ 1.2 m | ≥ 1.5 m |
| Ellipse lateral | ≥ 0.25 m | ≥ 0.3 m |

### 5.11 Pose Instability Detector

**Path**: `universe/autoware_universe/localization/autoware_pose_instability_detector/`
**Frequency**: 2 Hz (0.5 s timer)

Dead-reckons from twist history and compares against EKF output. Flags ERROR
if discrepancy exceeds velocity-dependent thresholds:

| Dimension | Tolerance |
|-----------|-----------|
| Longitudinal | 0.11 m + velocity × scale × dt |
| Lateral | 0.11 m + velocity × scale × dt |
| Vertical | 0.5 m |
| Yaw | 0.0175 rad + angular_vel × scale × dt |

### 5.12 Diagnostic Graph Aggregator

**Path**: `universe/autoware_universe/system/autoware_diagnostic_graph_aggregator/`
**Frequency**: 10 Hz

Aggregates `/diagnostics` from all subsystems into a dependency graph.
Maps diagnostic health to mode availability:

| Mode ID | Path | Description |
|---------|------|-------------|
| 1001 | `/autoware/modes/stop` | STOP mode |
| 1002 | `/autoware/modes/autonomous` | AUTONOMOUS mode |
| 1003 | `/autoware/modes/local` | LOCAL mode |
| 1004 | `/autoware/modes/remote` | REMOTE mode |
| 2001 | `/autoware/modes/emergency_stop` | EMERGENCY_STOP MRM |
| 2002 | `/autoware/modes/comfortable_stop` | COMFORTABLE_STOP MRM |
| 2003 | `/autoware/modes/pull_over` | PULL_OVER MRM |

If a diagnostic node is OK → mode AVAILABLE. Otherwise → NOT AVAILABLE.
This feeds into MRM handler's behavior selection.

## 6. Safety Island Candidates (Ranked)

### Tier 1: Minimal Viable Safety Island

These components have simple logic, strict timing requirements, and direct safety impact.
Combined, they form the **minimum viable safety island** on a single STM32F4.

| Component | Lines of Logic | Frequency | MCU Feasible |
|-----------|---------------|-----------|--------------|
| Heartbeat watchdog | ~30 | 10 Hz | Trivial |
| Emergency stop operator | ~50 | 30 Hz | Trivial |
| Comfortable stop operator | ~40 | 10 Hz | Trivial |
| Vehicle command gate (filter only) | ~300 | 10 Hz | Yes |

**nano-ros mapping**: 2 Subscribers (control cmd + heartbeat) + 1 Publisher (filtered cmd)
+ RTIC timer for watchdog. PollingExecutor at 30 Hz. LifecyclePollingNode for state management.

### Tier 2: Independent State Estimation

| Component | Lines of Logic | Frequency | MCU Feasible |
|-----------|---------------|-----------|--------------|
| Gyro odometer | ~200 | 100 Hz | Yes (FPU needed) |
| Control validator (subset) | ~300 | 10 Hz | Yes |
| PID longitudinal controller | ~500 | 10 Hz | Yes |
| Localization error monitor | ~100 | Event | Yes |

### Tier 3: Cross-Check (Requires Cortex-M4F+)

| Component | Lines of Logic | Frequency | MCU Feasible |
|-----------|---------------|-----------|--------------|
| Reduced EKF (2D) | ~400 | 50 Hz | Cortex-M4F with FPU |
| Pose instability detector | ~300 | 2 Hz | Yes |
| Pure pursuit (MPC fallback) | ~200 | 10 Hz | Yes |

### NOT Suitable for MCU

| Component | Reason |
|-----------|--------|
| MPC lateral controller | 200×200 matrices, OSQP solver |
| NDT scan matcher | PCL, multi-threaded, point clouds |
| Perception (any) | ML inference |
| Full diagnostic graph | Dynamic graph, string processing |

## 7. Proposed Architecture

```
┌───────────────────────────────────────────────────────────┐
│  Autoware Main Stack (x86/ARM64, Linux, ROS 2)           │
│  Perception · Planning · Full Localization · MPC · UI    │
└────────────────────┬──────────────────────────────────────┘
                     │ zenoh (Ethernet)
                     │
        ┌────────────┴────────────┐
        │                         │
┌───────▼──────────────┐  ┌──────▼───────────────────┐
│  Safety Island A     │  │  Safety Island B          │
│  STM32F4 + RTIC      │  │  STM32F4 + RTIC           │
│                      │  │                           │
│  Watchdog (10 Hz)    │  │  Gyro Odometer (100 Hz)   │
│  Cmd Gate (10 Hz)    │  │  Control Validator (10 Hz) │
│  E-Stop Op (30 Hz)   │  │  Localization Monitor     │
│  Comfort Stop (10Hz) │  │  Pose Instability (2 Hz)  │
│  PID Longitudinal    │  │  Reduced EKF (50 Hz)      │
│                      │  │                           │
│  nano-ros + smoltcp  │  │  nano-ros + smoltcp       │
│  PollingExecutor     │  │  PollingExecutor          │
│  LifecyclePollingNode│  │  LifecyclePollingNode     │
└───────┬──────────────┘  └──────┬────────────────────┘
        │ Filtered commands       │ Diagnostics/MRM trigger
        ▼                         ▼
  Vehicle Actuators         Island A (trigger e-stop)
```

**Island A** (actuation path): All control commands pass through it.
If the main stack dies (no heartbeat for 500 ms), Island A autonomously
applies emergency stop. Runs the command gate filter at all times.

**Island B** (monitoring): Independently estimates vehicle state from
IMU + wheel encoders. Validates the main stack's control outputs.
If discrepancy detected, sends MRM trigger to Island A.

## 8. nano-ros Readiness

| Capability Needed | Status | Gap |
|-------------------|--------|-----|
| Pub/Sub 100+ Hz | Working | None |
| Service server | Working | None |
| ROS 2 interop (rmw_zenoh) | Working | None |
| Lifecycle state machine (no_std) | Working | None |
| STM32F4 BSP | Working | None |
| RTIC task integration | Designed | spin::Mutex → critical section migration |
| CAN bus transport | Trait exists | Backend not implemented |
| WCET formal analysis | Framework documented | No published bounds |

## 9. Recommended First Implementation

**Target**: MRM Emergency Stop + Watchdog on STM32F4

```rust
// Pseudo-code for minimum viable safety island
#[rtic::app(device = stm32f4xx_hal::pac)]
mod app {
    // 10 Hz: poll zenoh, check heartbeat
    #[task(priority = 2)]
    fn poll_network(cx: poll_network::Context) {
        executor.spin_once(100);  // 100ms budget
        if heartbeat_age > Duration::from_millis(500) {
            trigger_emergency_stop();
        }
    }

    // 30 Hz: publish braking command when in e-stop
    #[task(priority = 3)]
    fn emergency_stop_loop(cx: emergency_stop_loop::Context) {
        if state == EStopActive {
            let a = max(prev_accel + JERK * dt, TARGET_ACCEL);
            let v = max(prev_vel + prev_accel * dt, 0.0);
            publish_control(v, a);
        }
    }
}
```

Constants: `TARGET_ACCEL = -2.5 m/s²`, `JERK = -1.5 m/s³`, `HEARTBEAT_TIMEOUT = 500 ms`.

This exercises the full nano-ros embedded stack and is immediately useful as a
fail-safe for any Autoware deployment.
