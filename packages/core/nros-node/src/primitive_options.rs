//! Per-entity creation options — rclrs's `PrimitiveOptions` /
//! `IntoPrimitiveOptions` (phase-515 R1).
//!
//! `node.create_publisher::<M>("chatter".keep_last(10).reliable())` reads as it
//! does in rclrs: a bare name, or a name with QoS steps chained onto it. Each
//! step records ONE policy; [`PrimitiveOptions::apply_to`] folds the recorded
//! policies over the entity kind's default profile at the create site, so a
//! policy nobody named keeps that kind's default (topics for a publisher,
//! services for a service).

use nros_rmw::{
    QoSDurabilityPolicy, QoSHistoryPolicy, QoSLivelinessPolicy, QoSProfile, QoSReliabilityPolicy,
};

/// rclrs's `PrimitiveOptions`: an entity's name plus the QoS policies its
/// create site stated.
///
/// ADOPT-BOUNDED against rclrs: the duration-valued policies hold whole
/// milliseconds (the unit [`QoSProfile`] records, `0` = infinite) where rclrs
/// holds a `QoSDuration` enum, so `deadline` / `lifespan` / `liveliness_lease`
/// take a `core::time::Duration` and the infinite case is
/// [`no_deadline`](IntoPrimitiveOptions::no_deadline) /
/// [`infinite_lifespan`](IntoPrimitiveOptions::infinite_lifespan). History and
/// depth are two fields because [`QoSProfile`] keeps them apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrimitiveOptions<'a> {
    /// The entity's (source) name: topic or service.
    pub name: &'a str,
    /// History policy, if stated.
    pub history: Option<QoSHistoryPolicy>,
    /// Queue depth, if stated (`keep_last(n)` states both).
    pub depth: Option<u32>,
    /// Reliability policy, if stated.
    pub reliability: Option<QoSReliabilityPolicy>,
    /// Durability policy, if stated.
    pub durability: Option<QoSDurabilityPolicy>,
    /// Deadline in milliseconds (`0` = none), if stated.
    pub deadline_ms: Option<u32>,
    /// Lifespan in milliseconds (`0` = infinite), if stated.
    pub lifespan_ms: Option<u32>,
    /// Liveliness policy, if stated.
    pub liveliness: Option<QoSLivelinessPolicy>,
    /// Liveliness lease in milliseconds (`0` = infinite), if stated.
    pub liveliness_lease_ms: Option<u32>,
    /// Skip the ROS topic-name prefix, if stated.
    pub avoid_ros_namespace_conventions: Option<bool>,
}

impl<'a> PrimitiveOptions<'a> {
    /// Options naming `name` and stating no policy.
    pub const fn new(name: &'a str) -> Self {
        Self {
            name,
            history: None,
            depth: None,
            reliability: None,
            durability: None,
            deadline_ms: None,
            lifespan_ms: None,
            liveliness: None,
            liveliness_lease_ms: None,
            avoid_ros_namespace_conventions: None,
        }
    }

    /// rclrs's `apply_to`: overwrite each policy this value states, leave the
    /// rest of `qos` as it is.
    pub fn apply_to(&self, qos: &mut QoSProfile) {
        if let Some(v) = self.history {
            qos.history = v;
        }
        if let Some(v) = self.depth {
            qos.depth = v;
        }
        if let Some(v) = self.reliability {
            qos.reliability = v;
        }
        if let Some(v) = self.durability {
            qos.durability = v;
        }
        if let Some(v) = self.deadline_ms {
            qos.deadline_ms = v;
        }
        if let Some(v) = self.lifespan_ms {
            qos.lifespan_ms = v;
        }
        if let Some(v) = self.liveliness {
            qos.liveliness_kind = v;
        }
        if let Some(v) = self.liveliness_lease_ms {
            qos.liveliness_lease_ms = v;
        }
        if let Some(v) = self.avoid_ros_namespace_conventions {
            qos.avoid_ros_namespace_conventions = v;
        }
    }

    /// `default` with this value's stated policies applied — what a create
    /// site resolves.
    pub fn resolve(&self, default: QoSProfile) -> QoSProfile {
        let mut qos = default;
        self.apply_to(&mut qos);
        qos
    }
}

/// The milliseconds a duration-valued policy records: saturating, and a
/// non-zero duration under 1 ms rounded UP, because `0` means infinite.
fn duration_ms(d: core::time::Duration) -> u32 {
    QoSProfile::new().deadline_duration(d).deadline_ms
}

/// rclrs's `IntoPrimitiveOptions`: anything an entity can be created from — a
/// bare name or a [`PrimitiveOptions`] — with rclrs's QoS steps chained onto
/// it (`"chatter".keep_last(10).reliable()`).
///
/// ADOPT-BOUNDED: see [`PrimitiveOptions`] for the duration-valued steps.
pub trait IntoPrimitiveOptions<'a>: Sized {
    /// Convert to the full options value.
    fn into_primitive_options(self) -> PrimitiveOptions<'a>;

    /// State every policy of `profile`.
    fn qos(self, profile: QoSProfile) -> PrimitiveOptions<'a> {
        let mut o = self.into_primitive_options();
        o.history = Some(profile.history);
        o.depth = Some(profile.depth);
        o.reliability = Some(profile.reliability);
        o.durability = Some(profile.durability);
        o.deadline_ms = Some(profile.deadline_ms);
        o.lifespan_ms = Some(profile.lifespan_ms);
        o.liveliness = Some(profile.liveliness_kind);
        o.liveliness_lease_ms = Some(profile.liveliness_lease_ms);
        o.avoid_ros_namespace_conventions = Some(profile.avoid_ros_namespace_conventions);
        o
    }

    /// [`qos`](Self::qos) with [`QoSProfile::topics_default`].
    fn topics_qos(self) -> PrimitiveOptions<'a> {
        self.qos(QoSProfile::topics_default())
    }

    /// [`qos`](Self::qos) with [`QoSProfile::sensor_data_default`].
    fn sensor_data_qos(self) -> PrimitiveOptions<'a> {
        self.qos(QoSProfile::sensor_data_default())
    }

    /// [`qos`](Self::qos) with [`QoSProfile::services_default`].
    fn services_qos(self) -> PrimitiveOptions<'a> {
        self.qos(QoSProfile::services_default())
    }

    /// [`qos`](Self::qos) with [`QoSProfile::system_default`].
    fn system_qos(self) -> PrimitiveOptions<'a> {
        self.qos(QoSProfile::system_default())
    }

    /// State the history policy.
    fn history(self, history: QoSHistoryPolicy) -> PrimitiveOptions<'a> {
        let mut o = self.into_primitive_options();
        o.history = Some(history);
        o
    }

    /// Keep the last `depth` samples.
    fn keep_last(self, depth: u32) -> PrimitiveOptions<'a> {
        let mut o = self.into_primitive_options();
        o.history = Some(QoSHistoryPolicy::KeepLast);
        o.depth = Some(depth);
        o
    }

    /// Keep every sample.
    fn keep_all(self) -> PrimitiveOptions<'a> {
        self.history(QoSHistoryPolicy::KeepAll)
    }

    /// State the reliability policy.
    fn reliability(self, reliability: QoSReliabilityPolicy) -> PrimitiveOptions<'a> {
        let mut o = self.into_primitive_options();
        o.reliability = Some(reliability);
        o
    }

    /// Reliable delivery.
    fn reliable(self) -> PrimitiveOptions<'a> {
        self.reliability(QoSReliabilityPolicy::Reliable)
    }

    /// Best-effort delivery.
    fn best_effort(self) -> PrimitiveOptions<'a> {
        self.reliability(QoSReliabilityPolicy::BestEffort)
    }

    /// State the durability policy.
    fn durability(self, durability: QoSDurabilityPolicy) -> PrimitiveOptions<'a> {
        let mut o = self.into_primitive_options();
        o.durability = Some(durability);
        o
    }

    /// Volatile durability.
    fn volatile(self) -> PrimitiveOptions<'a> {
        self.durability(QoSDurabilityPolicy::Volatile)
    }

    /// Transient-local durability.
    fn transient_local(self) -> PrimitiveOptions<'a> {
        self.durability(QoSDurabilityPolicy::TransientLocal)
    }

    /// The lifespan. ADOPT-BOUNDED: a `Duration`, not rclrs's `QoSDuration`.
    fn lifespan(self, lifespan: core::time::Duration) -> PrimitiveOptions<'a> {
        let mut o = self.into_primitive_options();
        o.lifespan_ms = Some(duration_ms(lifespan));
        o
    }

    /// rclrs's `lifespan_duration`, the same as [`lifespan`](Self::lifespan).
    fn lifespan_duration(self, duration: core::time::Duration) -> PrimitiveOptions<'a> {
        self.lifespan(duration)
    }

    /// Samples never expire.
    fn infinite_lifespan(self) -> PrimitiveOptions<'a> {
        let mut o = self.into_primitive_options();
        o.lifespan_ms = Some(0);
        o
    }

    /// The deadline. ADOPT-BOUNDED: a `Duration`, not rclrs's `QoSDuration`.
    fn deadline(self, deadline: core::time::Duration) -> PrimitiveOptions<'a> {
        let mut o = self.into_primitive_options();
        o.deadline_ms = Some(duration_ms(deadline));
        o
    }

    /// rclrs's `deadline_duration`, the same as [`deadline`](Self::deadline).
    fn deadline_duration(self, duration: core::time::Duration) -> PrimitiveOptions<'a> {
        self.deadline(duration)
    }

    /// No deadline.
    fn no_deadline(self) -> PrimitiveOptions<'a> {
        let mut o = self.into_primitive_options();
        o.deadline_ms = Some(0);
        o
    }

    /// State the liveliness policy.
    fn liveliness(self, liveliness: QoSLivelinessPolicy) -> PrimitiveOptions<'a> {
        let mut o = self.into_primitive_options();
        o.liveliness = Some(liveliness);
        o
    }

    /// Automatic liveliness.
    fn liveliness_automatic(self) -> PrimitiveOptions<'a> {
        self.liveliness(QoSLivelinessPolicy::Automatic)
    }

    /// Manual-by-topic liveliness, rclrs's one manual kind.
    fn liveliness_manual(self) -> PrimitiveOptions<'a> {
        self.liveliness(QoSLivelinessPolicy::ManualByTopic)
    }

    /// The liveliness lease. ADOPT-BOUNDED: a `Duration`, not rclrs's
    /// `QoSDuration`.
    fn liveliness_lease(self, lease: core::time::Duration) -> PrimitiveOptions<'a> {
        let mut o = self.into_primitive_options();
        o.liveliness_lease_ms = Some(duration_ms(lease));
        o
    }

    /// rclrs's `liveliness_lease_duration`, the same as
    /// [`liveliness_lease`](Self::liveliness_lease).
    fn liveliness_lease_duration(self, duration: core::time::Duration) -> PrimitiveOptions<'a> {
        self.liveliness_lease(duration)
    }

    /// Skip the ROS topic-name prefix.
    fn avoid_ros_namespace_conventions(self) -> PrimitiveOptions<'a> {
        let mut o = self.into_primitive_options();
        o.avoid_ros_namespace_conventions = Some(true);
        o
    }
}

/// rclrs's own blanket impl: any borrowed string — `&str`, `&String`,
/// `&heapless::String<N>` — names an entity, as a `&str` parameter accepted by
/// deref before this took options.
impl<'a, T: core::borrow::Borrow<str> + ?Sized + 'a> IntoPrimitiveOptions<'a> for &'a T {
    fn into_primitive_options(self) -> PrimitiveOptions<'a> {
        PrimitiveOptions::new(self.borrow())
    }
}

impl<'a> IntoPrimitiveOptions<'a> for PrimitiveOptions<'a> {
    fn into_primitive_options(self) -> PrimitiveOptions<'a> {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bare_name_keeps_the_kinds_default() {
        let o = "chatter".into_primitive_options();
        assert_eq!(
            o.resolve(QoSProfile::services_default()),
            QoSProfile::services_default()
        );
    }

    #[test]
    fn steps_overwrite_only_what_they_name() {
        let o = "chatter".keep_last(3).best_effort().transient_local();
        let q = o.resolve(QoSProfile::topics_default());
        assert_eq!(q.history, QoSHistoryPolicy::KeepLast);
        assert_eq!(q.depth, 3);
        assert_eq!(q.reliability, QoSReliabilityPolicy::BestEffort);
        assert_eq!(q.durability, QoSDurabilityPolicy::TransientLocal);
        let d = QoSProfile::topics_default();
        assert_eq!(q.deadline_ms, d.deadline_ms);
        assert_eq!(q.liveliness_kind, d.liveliness_kind);
    }

    #[test]
    fn a_profile_step_states_every_policy_and_later_steps_win() {
        let q = "scan"
            .sensor_data_qos()
            .reliable()
            .resolve(QoSProfile::services_default());
        let mut want = QoSProfile::sensor_data_default();
        want.reliability = QoSReliabilityPolicy::Reliable;
        assert_eq!(q, want);
    }

    #[test]
    fn duration_steps_record_milliseconds_and_infinite_is_zero() {
        let q = "t"
            .deadline(core::time::Duration::from_millis(250))
            .lifespan(core::time::Duration::from_micros(10))
            .liveliness_lease_duration(core::time::Duration::from_secs(2))
            .resolve(QoSProfile::topics_default());
        assert_eq!(q.deadline_ms, 250);
        assert_eq!(
            q.lifespan_ms, 1,
            "a non-zero duration under 1 ms is not infinite"
        );
        assert_eq!(q.liveliness_lease_ms, 2000);
        let q = "t".no_deadline().infinite_lifespan().resolve(q);
        assert_eq!((q.deadline_ms, q.lifespan_ms), (0, 0));
    }

    #[test]
    fn liveliness_and_namespace_steps() {
        let q = "t"
            .liveliness_manual()
            .avoid_ros_namespace_conventions()
            .resolve(QoSProfile::topics_default());
        assert_eq!(q.liveliness_kind, QoSLivelinessPolicy::ManualByTopic);
        assert!(q.avoid_ros_namespace_conventions);
    }
}
