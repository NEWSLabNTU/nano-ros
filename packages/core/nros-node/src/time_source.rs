//! The `/clock` time source — phase-425 W3.
//!
//! ROS 2 drives simulated time by publishing `rosgraph_msgs/msg/Clock` on
//! `/clock`: a simulator does it while it steps, `ros2 bag play --clock` does it
//! while it replays. A node that subscribes installs each sample as the image's
//! ROS time, and everything reading a `ClockType::RosTime` clock — including a
//! [`TimerClockSource::Ros`](crate::executor::TimerClockSource) timer — then
//! follows the simulation instead of the wall.
//!
//! Without this the type existed and nothing drove it: `ClockType::RosTime` and
//! its override have been in `nros-core` since issue 0789, but the only way to
//! move them was for the program to call the setter itself.
//!
//! The entry point is [`NodeCtx::install_ros_time_source`](crate::executor::node::NodeCtx::install_ros_time_source).
//!
//! # What this is not
//!
//! It is not `rclcpp::TimeSource`. There are no jump callbacks, no per-clock
//! attachment and no clock thread: the override is process-global — ONE
//! simulated clock per image, the model `nros_core::Clock` already documents —
//! so there is nothing to attach to and nothing to fan out.
//!
//! # Cost
//!
//! One subscription: an entity slot plus an RX buffer. That is why `sim-time` is
//! a feature and not a default — an image that will never see a simulator should
//! not pay for it.

/// The topic ROS 2 publishes simulated time on.
pub const CLOCK_TOPIC: &str = "/clock";

/// The reserved parameter whose value attaches the time source — phase-425 W3b.
///
/// ROS 2 gives every node this parameter and treats it as a switch rather than a
/// value: nothing reads it, the client library acts on it. We do the same, at
/// `Executor::declare_parameter`, which is the one seam every language's
/// declaration path funnels through.
pub const USE_SIM_TIME_PARAM: &str = "use_sim_time";

/// Whether `/clock` samples are being installed.
///
/// Process-global for the same reason the override itself is: one simulated
/// clock per image. Defaults to TRUE so that an explicit
/// `install_ros_time_source()` needs no second call to arm it — a program that
/// asks for the source wants the source. `use_sim_time` toggles it from there.
static SIM_TIME_ACTIVE: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(true);

/// Whether an arriving `/clock` sample should be installed.
pub fn is_active() -> bool {
    SIM_TIME_ACTIVE.load(core::sync::atomic::Ordering::Relaxed)
}

/// Start or stop installing `/clock` samples.
///
/// Stopping does NOT clear the current override: a node that stops listening
/// keeps the last simulated time rather than jumping back to the wall clock,
/// which every ROS-time timer would otherwise have to absorb as a backwards
/// jump. Clearing is `Clock::clear_ros_time_override()`, and it is the caller's
/// decision because it is a visible time discontinuity.
pub fn set_active(active: bool) {
    SIM_TIME_ACTIVE.store(active, core::sync::atomic::Ordering::Relaxed);
}

/// A `/clock` sample as a nanosecond count, or `None` if it is not installable.
///
/// `builtin_interfaces/Time` is `(sec: i32, nanosec: u32)`; the override is a
/// single `i64`. A NEGATIVE total means a sample before the epoch, which the
/// setter rejects — and dropping it is the right answer rather than clamping: a
/// ROS-time clock with no override reads system time, which is what a node
/// receiving nonsense from a misconfigured publisher should see, instead of a
/// simulated clock pinned at zero.
pub(crate) fn override_nanos(sec: i32, nanosec: u32) -> Option<i64> {
    let nanos = (sec as i64)
        .saturating_mul(1_000_000_000)
        .saturating_add(nanosec as i64);
    (nanos >= 0).then_some(nanos)
}

#[cfg(test)]
mod tests {
    use super::override_nanos;

    #[test]
    fn a_clock_sample_becomes_nanoseconds() {
        assert_eq!(override_nanos(0, 0), Some(0));
        assert_eq!(override_nanos(1, 500_000_000), Some(1_500_000_000));
        // The nanosec field is unsigned and can exceed one second in a sloppy
        // publisher; carrying it is arithmetic, not validation.
        assert_eq!(override_nanos(2, 1_500_000_000), Some(3_500_000_000));
    }

    #[test]
    fn a_pre_epoch_sample_installs_nothing() {
        assert_eq!(override_nanos(-1, 0), None);
        // Saturating rather than wrapping: the minimum sec cannot become a
        // positive nanosecond count.
        assert_eq!(override_nanos(i32::MIN, u32::MAX), None);
    }
}
