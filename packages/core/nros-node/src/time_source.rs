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

/// How much of the executor's OWN elapsed time may pass, after the `/clock`
/// source is attached, before the silence is worth a line — phase-430 W1.
///
/// Five seconds: long enough that a simulator still coming up, or a bag whose
/// first sample is a second or two out, says nothing; short enough that a
/// misconfigured image says it before anyone has finished reading the boot log.
///
/// # Why this is measured in the executor's elapsed time, and not in spins or wall time
///
/// The three candidates are a spin COUNT, a WALL duration, and the first
/// ROS-time timer that comes due with no sample yet. None of them is free:
///
/// * A **spin count** is portable and needs no clock at all, and it means
///   nothing. A `spin_once(5ms)` loop and a busy poll differ by four orders of
///   magnitude, so any constant is right for one image and absurd for every
///   other. The number would have to be tuned per image, which is the folklore
///   this diagnostic exists to replace.
/// * A **wall duration** is the obvious choice and has the obvious flaw: on a
///   target running under simulation the wall clock is part of the fiction
///   too — Zephyr `native_sim` runs its uptime as fast as the host will let it
///   when idle, QEMU under `-icount` runs it as slowly as the model dictates —
///   so "five seconds" from an independent host clock is not five seconds of
///   anything the image experienced. Worse, an executor is allowed to have NO
///   clock at all (`Executor::now_us` returns `Option`, and it is `None` on a
///   freestanding build with no injected hook), so a wall threshold would make
///   the diagnostic silently unreachable on exactly the targets it is for.
/// * The **first ROS-time timer that comes due** is the semantically sharpest
///   trigger — it is the instant the misconfiguration becomes visible
///   behaviour — but it only covers images that HAVE a ROS-time timer. A node
///   that merely stamps messages off `Clock::ros_time()` is just as
///   misconfigured and would never hear a word.
///
/// So the threshold is the executor's own `delta_us`: the same per-spin
/// quantity every WALL timer on the same executor accumulates, with the same
/// "no clock, credit the requested timeout" fallback `spin_once` already
/// applies. Three properties follow, and they are why this is the right unit:
/// the diagnostic arrives after as much time as the image's own wall timers
/// believe has passed (so it is legible against them); it moves WITH the
/// target's simulated wall clock instead of against it; and a clockless image
/// is not exempt, because the fallback still advances it.
pub const SILENCE_WARN_US: u64 = 5_000_000;

/// The one-shot watch behind [`SILENCE_WARN_US`] — phase-430 W1.
///
/// An image told `use_sim_time = true` that never receives a `/clock` sample
/// looks exactly like one whose simulator is merely PAUSED: in both, every
/// ROS-time timer sits still and nothing is said. The two need different
/// answers from the operator, so the image has to distinguish them, and only
/// the image can — "no sample has EVER arrived" is not a question an outside
/// observer of the topic can answer about this subscriber.
///
/// Kept as a small state machine rather than two fields on the executor so the
/// policy — when it arms, when it gives up, when it must not repeat — is
/// testable without an executor, and so there is exactly one place to read it.
#[derive(Debug, Default)]
pub(crate) struct SilenceWatch {
    /// Whether the watch is counting. False both before the source is attached
    /// and after the question has been ANSWERED, either way.
    armed: bool,
    /// Executor-elapsed µs since the watch armed.
    elapsed_us: u64,
    /// How many times this watch has reported. The executor exposes it so a
    /// test can assert "exactly one" without grepping a log sink, and so an
    /// application can ask the same question the log line answers.
    reports: u32,
}

impl SilenceWatch {
    pub(crate) const fn new() -> Self {
        Self {
            armed: false,
            elapsed_us: 0,
            reports: 0,
        }
    }

    /// The `/clock` source was just attached: start counting, and clear the
    /// one-shot so a source that is detached and re-attached is watched again.
    ///
    /// Arming is what makes the interval start at ATTACH rather than at
    /// construction: before the subscription exists there is nothing to be
    /// silent, and counting from boot would fire on an image that spent six
    /// seconds bringing a transport up.
    pub(crate) fn arm(&mut self) {
        self.armed = true;
        self.elapsed_us = 0;
    }

    /// The source is no longer wanted (`use_sim_time` went false). Stop
    /// counting; a later re-arm starts the interval over.
    pub(crate) fn disarm(&mut self) {
        self.armed = false;
        self.elapsed_us = 0;
    }

    /// Credit `delta_us` of the executor's own elapsed time and answer whether
    /// THIS is the spin that should emit the diagnostic.
    ///
    /// `sample_seen` is [`Clock::is_ros_time_override_active`], i.e. "simulated
    /// time is being driven". A sample arriving at any point before the
    /// threshold disarms the watch permanently: the configuration is then
    /// PROVEN right, and a simulator that stops afterwards is a pause — the
    /// case this diagnostic must stay quiet about, because saying "no /clock"
    /// about a paused bag is exactly the false alarm that would teach everyone
    /// to ignore the line.
    ///
    /// Returns `true` at most once per arming; the report itself disarms, so
    /// the record cannot become the noise it warns about.
    ///
    /// [`Clock::is_ros_time_override_active`]: nros_core::clock::Clock::is_ros_time_override_active
    pub(crate) fn tick(&mut self, delta_us: u64, sample_seen: bool) -> bool {
        if !self.armed {
            return false;
        }
        if sample_seen {
            self.disarm();
            return false;
        }
        self.elapsed_us = self.elapsed_us.saturating_add(delta_us);
        if self.elapsed_us < SILENCE_WARN_US {
            return false;
        }
        self.armed = false;
        self.reports = self.reports.saturating_add(1);
        true
    }

    /// How many times this watch has reported.
    pub(crate) fn reports(&self) -> u32 {
        self.reports
    }
}

/// Say that `use_sim_time` is on and `/clock` has never spoken — phase-430 W1.
///
/// `nros_log`, never stdio: this is reached on `no_std` targets and inside
/// Zephyr `native_sim`, where a Rust `std` stdio call is FATAL (issue 0589).
///
/// BUDGET: `nros_log`'s call-site format buffer is 256 bytes by default and
/// overflow TRUNCATES with a `…`, so a line that explains itself past the
/// budget delivers exactly the folklore it replaces (the lesson
/// `arena::report_arena_headroom` paid for). Both identifiers an operator has
/// to act on — the parameter and the topic — are in the first clause, and the
/// reasoning lives here rather than on the wire.
#[cold]
pub(crate) fn report_silence(topic: &str) {
    nros_log::log_warn!(
        nros_log::get_logger("nros"),
        "{} is true but no {} sample has arrived in {}s: ROS-time timers are \
         running on SYSTEM time meanwhile. Publish rosgraph_msgs/msg/Clock \
         (ros2 bag play --clock), or set {} false.",
        USE_SIM_TIME_PARAM,
        topic,
        SILENCE_WARN_US / 1_000_000,
        USE_SIM_TIME_PARAM
    );
}

#[cfg(test)]
mod tests {
    use super::{SILENCE_WARN_US, SilenceWatch, override_nanos};

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

    /// phase-430 W1 — the watch says nothing until it is ARMED, however much
    /// time passes. Before the `/clock` source is attached there is nothing to
    /// be silent about, and an image that spends six seconds bringing a
    /// transport up must not be accused of a misconfiguration it does not have.
    #[test]
    fn an_unarmed_watch_never_reports() {
        let mut watch = SilenceWatch::new();
        for _ in 0..10 {
            assert!(!watch.tick(SILENCE_WARN_US, false));
        }
        assert_eq!(watch.reports(), 0);
    }

    /// The interval, and the one-shot: it fires when the executor's own elapsed
    /// time reaches the threshold, and NOT one microsecond earlier.
    #[test]
    fn a_silent_source_reports_once_at_the_threshold() {
        let mut watch = SilenceWatch::new();
        watch.arm();
        assert!(
            !watch.tick(SILENCE_WARN_US - 1, false),
            "one microsecond short of the threshold is not the threshold"
        );
        assert!(
            watch.tick(1, false),
            "the threshold must fire on the spin that reaches it"
        );
        assert_eq!(watch.reports(), 1);
        // The record must not become the noise it warns about: the report
        // disarms, so every later spin is silent even though the condition it
        // described is still true.
        for _ in 0..100 {
            assert!(!watch.tick(SILENCE_WARN_US, false));
        }
        assert_eq!(watch.reports(), 1);
    }

    /// A sample arriving before the interval answers the question, permanently.
    ///
    /// Permanently, and not "until it stops": a simulator that publishes and
    /// then pauses is the case this diagnostic exists to be DISTINGUISHED from,
    /// so warning about it would be the false alarm that teaches everyone to
    /// ignore the line.
    #[test]
    fn a_sample_before_the_threshold_silences_the_watch_for_good() {
        let mut watch = SilenceWatch::new();
        watch.arm();
        assert!(!watch.tick(SILENCE_WARN_US - 1, false));
        assert!(
            !watch.tick(1, true),
            "a sample arrived; there is nothing to report"
        );
        for _ in 0..100 {
            assert!(
                !watch.tick(SILENCE_WARN_US, false),
                "/clock spoke once and then stopped -- that is a PAUSE, and a \
                 pause is the state this diagnostic must not shout about"
            );
        }
        assert_eq!(watch.reports(), 0);
    }

    /// Re-arming starts the interval over, so a source detached and re-attached
    /// is watched again — and the elapsed time from the first arming does not
    /// carry over and fire the new watch on its first spin.
    #[test]
    fn re_arming_restarts_the_interval() {
        let mut watch = SilenceWatch::new();
        watch.arm();
        assert!(watch.tick(SILENCE_WARN_US, false));
        assert_eq!(watch.reports(), 1);

        watch.disarm();
        watch.arm();
        assert!(
            !watch.tick(SILENCE_WARN_US - 1, false),
            "the second watch inherited elapsed time from the first"
        );
        assert!(watch.tick(1, false));
        assert_eq!(watch.reports(), 2);
    }
}
