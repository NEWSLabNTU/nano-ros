//! Zenoh key expression generation for RMW types
//!
//! Extension traits that add zenoh-specific key expression methods
//! to the middleware-agnostic `TopicInfo`, `ServiceInfo`, and `QoSProfile` types.

use nros_rmw::{
    DURATION_BEST_AVAILABLE_MS, DURATION_INFINITE_MS, QoSDurabilityPolicy, QoSHistoryPolicy,
    QoSLivelinessPolicy, QoSProfile, QoSReliabilityPolicy, ServiceInfo, TopicInfo,
};

/// Extension trait for generating zenoh key expressions from `TopicInfo`
pub trait TopicKeyExpr {
    /// Generate the full topic key in rmw_zenoh format
    ///
    /// - Humble (default): `<domain_id>/<topic_name>/<type_name>/TypeHashNotSupported`
    /// - Iron/Jazzy/Rolling (`ros-{iron,jazzy,rolling}`): `<domain_id>/<topic_name>/<type_name>/<type_hash>`
    fn to_key<const N: usize>(&self) -> heapless::String<N>;

    /// Generate a wildcard topic key for subscribing
    /// Format: `<domain_id>/<topic_name>/<type_name>/*`
    fn to_key_wildcard<const N: usize>(&self) -> heapless::String<N>;
}

impl TopicKeyExpr for TopicInfo<'_> {
    fn to_key<const N: usize>(&self) -> heapless::String<N> {
        let mut key = heapless::String::new();
        let topic_stripped = self.name.trim_matches('/');
        #[cfg(not(any(feature = "ros-iron", feature = "ros-jazzy")))]
        let _ = core::fmt::write(
            &mut key,
            format_args!(
                "{}/{}/{}/TypeHashNotSupported",
                self.domain_id,
                topic_stripped,
                DdsTypeName(self.type_name)
            ),
        );
        #[cfg(any(feature = "ros-iron", feature = "ros-jazzy"))]
        let _ = core::fmt::write(
            &mut key,
            format_args!(
                "{}/{}/{}/{}",
                self.domain_id,
                topic_stripped,
                DdsTypeName(self.type_name),
                self.type_hash
            ),
        );
        key
    }

    fn to_key_wildcard<const N: usize>(&self) -> heapless::String<N> {
        let mut key = heapless::String::new();
        let topic_stripped = self.name.trim_matches('/');
        let _ = core::fmt::write(
            &mut key,
            format_args!(
                "{}/{}/{}/*",
                self.domain_id,
                topic_stripped,
                DdsTypeName(self.type_name)
            ),
        );
        key
    }
}

/// Render a service type name in the DDS form rmw_zenoh puts on the wire.
///
/// nano-ros' raw service API takes the ROS-style `pkg/srv/Type` (every call
/// site in this tree passes that form, and it is what `ros2 service call`
/// prints), but rmw_zenoh keys carry the mangled `pkg::srv::dds_::Type_`.
/// Leaving it unmangled does not merely look wrong: `/` is a zenoh keyexpr
/// SEPARATOR, so `example_interfaces/srv/AddTwoInts` splits the key into three
/// extra segments and nothing lines up with the fields a peer expects. That is
/// why a service registered from this stack was invisible to `ros2 service
/// list` even with the domain correct (issue 0824).
///
/// Verified against a native `demo_nodes_cpp add_two_ints_server` on
/// rmw_zenoh, which declares:
///   `…/SS/%/%/add_two_ints_server/%add_two_ints/example_interfaces::srv::dds_::AddTwoInts_/…`
///
/// A name that already contains `::` is passed through untouched, so callers
/// that supply the mangled form (generated code does) are unaffected.
/// Anything that is not exactly `a/b/c` is passed through as well rather than
/// mangled into nonsense.
pub(crate) struct DdsTypeName<'a>(pub &'a str);

impl core::fmt::Display for DdsTypeName<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let s = self.0;
        if s.contains("::") {
            return f.write_str(s);
        }
        let mut parts = s.split('/');
        match (parts.next(), parts.next(), parts.next(), parts.next()) {
            (Some(pkg), Some(kind), Some(name), None)
                if !pkg.is_empty() && !kind.is_empty() && !name.is_empty() =>
            {
                // The action path derives sub-types by appending to the base
                // ("Fibonacci" -> "Fibonacci_SendGoal_"), so the name can
                // ALREADY end in the trailing underscore. Appending a second
                // one put "Fibonacci_SendGoal__" on the wire against a native
                // peer's "Fibonacci_SendGoal_".
                if name.ends_with('_') {
                    write!(f, "{pkg}::{kind}::dds_::{name}")
                } else {
                    write!(f, "{pkg}::{kind}::dds_::{name}_")
                }
            }
            _ => f.write_str(s),
        }
    }
}

/// Extension trait for generating zenoh key expressions from `ServiceInfo`
pub trait ServiceKeyExpr {
    /// Generate the service key in rmw_zenoh format
    ///
    /// - Humble (default): `<domain_id>/<service_name>/<type_name>/TypeHashNotSupported`
    /// - Iron/Jazzy/Rolling (`ros-{iron,jazzy,rolling}`): `<domain_id>/<service_name>/<type_name>/<type_hash>`
    fn to_key<const N: usize>(&self) -> heapless::String<N>;

    /// Generate a wildcard service key for client queries
    /// Format: `<domain_id>/<service_name>/<type_name>/*`
    fn to_key_wildcard<const N: usize>(&self) -> heapless::String<N>;
}

impl ServiceKeyExpr for ServiceInfo<'_> {
    fn to_key<const N: usize>(&self) -> heapless::String<N> {
        let mut key = heapless::String::new();
        let service_stripped = self.name.trim_matches('/');
        #[cfg(not(any(feature = "ros-iron", feature = "ros-jazzy")))]
        let _ = core::fmt::write(
            &mut key,
            format_args!(
                "{}/{}/{}/TypeHashNotSupported",
                self.domain_id,
                service_stripped,
                DdsTypeName(self.type_name)
            ),
        );
        #[cfg(any(feature = "ros-iron", feature = "ros-jazzy"))]
        let _ = core::fmt::write(
            &mut key,
            format_args!(
                "{}/{}/{}/{}",
                self.domain_id,
                service_stripped,
                DdsTypeName(self.type_name),
                self.type_hash
            ),
        );
        key
    }

    fn to_key_wildcard<const N: usize>(&self) -> heapless::String<N> {
        let mut key = heapless::String::new();
        let service_stripped = self.name.trim_matches('/');
        let _ = core::fmt::write(
            &mut key,
            format_args!(
                "{}/{}/{}/*",
                self.domain_id,
                service_stripped,
                DdsTypeName(self.type_name)
            ),
        );
        key
    }
}

/// Extension trait for generating zenoh QoS liveliness strings from `QoSProfile`
pub trait QosKeyExpr {
    /// Convert QoS settings to rmw_zenoh liveliness token format string.
    ///
    /// Format: `reliability:durability:history,depth:deadline:lifespan:liveliness,lease:avoid_ros_namespace_conventions`
    ///
    /// rmw_zenoh encoding:
    /// - reliability: SYSTEM_DEFAULT=0, RELIABLE=1, BEST_EFFORT=2
    /// - durability: SYSTEM_DEFAULT=0, TRANSIENT_LOCAL=1, VOLATILE=2
    /// - history: SYSTEM_DEFAULT=0, KEEP_LAST=1, KEEP_ALL=2
    fn to_qos_string<const N: usize>(&self) -> heapless::String<N>;
}

// nros-qos-discovery-only: this serialiser puts a profile into the `@ros2_lv`
// keyexpr for a peer's graph parser. It APPLIES nothing — phase-428 W9 found
// four policies whose only read in the whole backend was this function, read
// as evidence that the shim honoured them. `check-qos-mask-derivation` does
// not count a read inside this region, so a policy cannot earn a mask bit by
// being mentioned to somebody else.
//
// The profile reaching here is the GRANTED one (`shim/qos.rs`), so what a peer
// reads is what this image does.
impl QosKeyExpr for QoSProfile {
    fn to_qos_string<const N: usize>(&self) -> heapless::String<N> {
        let mut s = heapless::String::new();

        // issue 0829 — the sentinel gets upstream's own digit, 0, rather than
        // being folded onto a concrete policy. It should never arrive here:
        // `ZenohSession::create_{publisher,subscription}` resolve before
        // deriving anything from the profile, precisely so a ROS peer parsing
        // this token out of the graph never reads `0:0:0,0` as a policy. Emit
        // the honest digit anyway, so a leak reads as "unstated" to that peer
        // instead of as a reliability we never chose.
        let reliability = match self.reliability {
            QoSReliabilityPolicy::SystemDefault => 0,
            QoSReliabilityPolicy::Reliable => 1,
            QoSReliabilityPolicy::BestEffort => 2,
            // issue 1437 — a read-back sentinel, which cannot reach a token: a
            // token is derived from `admit`'s GRANT, and `admit` never grants
            // one. Upstream's `rmw_zenoh_cpp` has no digit for it either, so 0
            // ("unstated") is the least wrong thing to emit if it ever did.
            QoSReliabilityPolicy::Unknown => 0,
            // issue 1973 — a REQUEST value, resolved by `create_*` before the
            // grant this token is derived from, so it cannot arrive either.
            // Upstream never puts it in a token (it resolves first too), so 0
            // ("unstated") is again the least wrong digit.
            QoSReliabilityPolicy::BestAvailable => 0,
        };

        let durability = match self.durability {
            QoSDurabilityPolicy::SystemDefault => 0,
            QoSDurabilityPolicy::TransientLocal => 1,
            QoSDurabilityPolicy::Volatile => 2,
            // See the reliability arm.
            QoSDurabilityPolicy::Unknown => 0,
            QoSDurabilityPolicy::BestAvailable => 0,
        };

        let history = match self.history {
            QoSHistoryPolicy::SystemDefault => 0,
            QoSHistoryPolicy::KeepLast => 1,
            QoSHistoryPolicy::KeepAll => 2,
            // See the reliability arm.
            QoSHistoryPolicy::Unknown => 0,
        };

        // Issue 1495 — CANONICAL form: a field equal to `rmw_zenoh_cpp`'s own
        // default is written EMPTY, exactly as upstream's `qos_to_keyexpr`
        // writes it. Parse-equivalent either way (upstream's `keyexpr_to_qos`
        // reads an empty field as that default), so no peer reads a different
        // profile. What it changes is the STRING, and the string is an
        // identity: a peer derives our entity gid by parsing this token and
        // re-serialising it, so a token that does not round-trip through
        // upstream's parser byte for byte hashes to a gid we never stamped.
        // Measured: `1:2:1,10` here made `ros2 topic info --verbose` print a
        // gid for our publisher that matched none of our samples.
        fn field<const N: usize>(s: &mut heapless::String<N>, v: u32, default: u32, sep: char) {
            if v != default {
                let _ = core::fmt::write(s, format_args!("{v}"));
            }
            let _ = s.push(sep);
        }
        field(&mut s, reliability, UPSTREAM_DEFAULT_RELIABILITY, ':');
        field(&mut s, durability, UPSTREAM_DEFAULT_DURABILITY, ':');
        field(&mut s, history, UPSTREAM_DEFAULT_HISTORY, ',');
        if self.depth != UPSTREAM_DEFAULT_DEPTH {
            let _ = core::fmt::write(&mut s, format_args!("{}", self.depth));
        }
        // Deadline, lifespan and liveliness are never stated, so they read as
        // upstream's defaults — which is also what upstream writes for them.
        let _ = s.push_str(":,:,:,,");

        s
    }
}

/// Decode a liveliness token's QoS chunk — issue 1973.
///
/// The inverse of [`QosKeyExpr::to_qos_string`], in `rmw_zenoh_cpp`'s grammar
/// (`keyexpr_to_qos`): six `:`-separated fields
/// `rel:dur:hist,depth:dl_sec,dl_nsec:ls_sec,ls_nsec:liv,lease_sec,lease_nsec`,
/// where an EMPTY field means upstream's `QoS::default_qos()` value (reliable,
/// volatile, keep-last(42), automatic liveliness, infinite windows — the
/// constants below, plus `RMW_ZENOH_DEFAULT_LIVELINESS = AUTOMATIC`).
///
/// Windows land on our u32 milliseconds the way [`nros_rmw::duration_to_qos_ms`]
/// lowers them (a sub-millisecond remainder ceils); one too large for the
/// finite range — upstream's `RMW_DURATION_INFINITE` among them — reads as
/// [`DURATION_INFINITE_MS`]. A digit this grammar does not define reads as that
/// policy's `Unknown`, and a chunk that does not parse at all as
/// [`QoSProfile::QOS_PROFILE_UNKNOWN`]: a peer's token is somebody else's
/// output, and "I cannot say" is the honest reading of one we cannot read.
pub fn qos_from_keyexpr(s: &str) -> QoSProfile {
    fn num(f: &str) -> Option<Option<u64>> {
        if f.is_empty() {
            Some(None)
        } else {
            f.parse::<u64>().ok().map(Some)
        }
    }
    fn window(f: &str) -> Option<u32> {
        let mut it = f.split(',');
        let (sec, nsec) = (it.next()?, it.next()?);
        if it.next().is_some() {
            return None;
        }
        let (sec, nsec) = (num(sec)?, num(nsec)?);
        Some(window_ms(
            sec.unwrap_or(0),
            nsec.unwrap_or(0),
            sec.is_none() && nsec.is_none(),
        ))
    }
    fn window_ms(sec: u64, nsec: u64, empty: bool) -> u32 {
        if empty {
            // `RMW_ZENOH_DEFAULT_{DEADLINE,LIFESPAN,…}` is RMW_DURATION_INFINITE.
            return DURATION_INFINITE_MS;
        }
        if sec == 0 && nsec == 0 {
            return 0;
        }
        let ms = (sec as u128) * 1000 + (nsec as u128).div_ceil(1_000_000);
        if ms >= DURATION_BEST_AVAILABLE_MS as u128 {
            DURATION_INFINITE_MS
        } else {
            ms as u32
        }
    }
    let parse = || -> Option<QoSProfile> {
        let mut parts = s.split(':');
        let rel = parts.next()?;
        let dur = parts.next()?;
        let hist = parts.next()?;
        let deadline = parts.next()?;
        let lifespan = parts.next()?;
        let liv = parts.next()?;
        if parts.next().is_some() {
            return None;
        }
        let mut q = QoSProfile::QOS_PROFILE_UNKNOWN;
        q.reliability = match num(rel)? {
            None | Some(1) => QoSReliabilityPolicy::Reliable,
            Some(0) => QoSReliabilityPolicy::SystemDefault,
            Some(2) => QoSReliabilityPolicy::BestEffort,
            Some(_) => QoSReliabilityPolicy::Unknown,
        };
        q.durability = match num(dur)? {
            None | Some(2) => QoSDurabilityPolicy::Volatile,
            Some(0) => QoSDurabilityPolicy::SystemDefault,
            Some(1) => QoSDurabilityPolicy::TransientLocal,
            Some(_) => QoSDurabilityPolicy::Unknown,
        };
        let (h, depth) = hist.split_once(',')?;
        q.history = match num(h)? {
            None | Some(1) => QoSHistoryPolicy::KeepLast,
            Some(0) => QoSHistoryPolicy::SystemDefault,
            Some(2) => QoSHistoryPolicy::KeepAll,
            Some(_) => QoSHistoryPolicy::Unknown,
        };
        q.depth = match num(depth)? {
            None => UPSTREAM_DEFAULT_DEPTH,
            Some(d) => u32::try_from(d).ok()?,
        };
        q.deadline_ms = window(deadline)?;
        q.lifespan_ms = window(lifespan)?;
        let (kind, lease) = liv.split_once(',')?;
        q.liveliness_kind = match num(kind)? {
            None | Some(1) => QoSLivelinessPolicy::Automatic,
            Some(0) => QoSLivelinessPolicy::None,
            Some(2) => QoSLivelinessPolicy::ManualByNode,
            Some(3) => QoSLivelinessPolicy::ManualByTopic,
            Some(_) => QoSLivelinessPolicy::Unknown,
        };
        q.liveliness_lease_ms = window(lease)?;
        q.avoid_ros_namespace_conventions = false;
        Some(q)
    };
    parse().unwrap_or(QoSProfile::QOS_PROFILE_UNKNOWN)
}

/// `rmw_zenoh_cpp`'s `QoS::default_qos()` (`detail/qos.cpp`, 0.1.9): the
/// values its `qos_to_keyexpr` elides. NOT `rmw_qos_profile_default` — the
/// depth is 42, upstream's own choice, and eliding 10 would make every
/// default-profile token mean depth 42 to a peer.
const UPSTREAM_DEFAULT_RELIABILITY: u32 = 1; // RELIABLE
const UPSTREAM_DEFAULT_DURABILITY: u32 = 2; // VOLATILE
const UPSTREAM_DEFAULT_HISTORY: u32 = 1; // KEEP_LAST
const UPSTREAM_DEFAULT_DEPTH: u32 = 42;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(not(any(feature = "ros-iron", feature = "ros-jazzy")))]
    fn test_topic_key_generation() {
        let topic =
            TopicInfo::new("/chatter", "std_msgs::msg::dds_::String_", "abc123").with_domain(42);
        let key: heapless::String<128> = topic.to_key();
        assert!(key.contains("42"));
        assert!(key.contains("chatter"));
        assert!(key.contains("TypeHashNotSupported"));
    }

    #[test]
    #[cfg(any(feature = "ros-iron", feature = "ros-jazzy"))]
    fn test_topic_key_generation_iron() {
        let topic = TopicInfo::new("/chatter", "std_msgs::msg::dds_::String_", "RIHS01_abc123")
            .with_domain(42);
        let key: heapless::String<128> = topic.to_key();
        assert!(key.contains("42"));
        assert!(key.contains("chatter"));
        assert!(key.contains("RIHS01_abc123"));
        assert!(!key.contains("TypeHashNotSupported"));
    }

    #[test]
    #[cfg(not(any(feature = "ros-iron", feature = "ros-jazzy")))]
    fn test_topic_info_to_key_humble() {
        let topic = TopicInfo::new(
            "/chatter",
            "std_msgs::msg::dds_::Int32_",
            "TypeHashNotSupported",
        );
        let key: heapless::String<128> = topic.to_key();
        assert_eq!(
            key.as_str(),
            "0/chatter/std_msgs::msg::dds_::Int32_/TypeHashNotSupported"
        );
    }

    #[test]
    #[cfg(any(feature = "ros-iron", feature = "ros-jazzy"))]
    fn test_topic_info_to_key_iron() {
        let topic = TopicInfo::new("/chatter", "std_msgs::msg::dds_::Int32_", "RIHS01_deadbeef");
        let key: heapless::String<128> = topic.to_key();
        assert_eq!(
            key.as_str(),
            "0/chatter/std_msgs::msg::dds_::Int32_/RIHS01_deadbeef"
        );
    }

    #[test]
    fn test_topic_info_to_key_wildcard() {
        let topic = TopicInfo::new(
            "/chatter",
            "std_msgs::msg::dds_::Int32_",
            "TypeHashNotSupported",
        );
        let key: heapless::String<128> = topic.to_key_wildcard();
        assert_eq!(key.as_str(), "0/chatter/std_msgs::msg::dds_::Int32_/*");
    }

    #[test]
    #[cfg(not(any(feature = "ros-iron", feature = "ros-jazzy")))]
    fn test_service_key() {
        let service = ServiceInfo::new(
            "/add_two_ints",
            "example_interfaces::srv::dds_::AddTwoInts",
            "TypeHashNotSupported",
        );
        let key: heapless::String<128> = service.to_key();
        assert!(key.contains("add_two_ints"));
        assert!(key.contains("TypeHashNotSupported"));
    }

    #[test]
    #[cfg(any(feature = "ros-iron", feature = "ros-jazzy"))]
    fn test_service_key_iron() {
        let service = ServiceInfo::new(
            "/add_two_ints",
            "example_interfaces::srv::dds_::AddTwoInts",
            "RIHS01_cafebabe",
        );
        let key: heapless::String<128> = service.to_key();
        assert!(key.contains("add_two_ints"));
        assert!(key.contains("RIHS01_cafebabe"));
        assert!(!key.contains("TypeHashNotSupported"));
    }

    #[test]
    fn test_qos_string_sensor_data() {
        let qos = QoSProfile::QOS_PROFILE_SENSOR_DATA;
        let s: heapless::String<32> = qos.to_qos_string();
        assert_eq!(s.as_str(), "2::,5:,:,:,,");
    }

    #[test]
    fn test_qos_string_default() {
        let qos = QoSProfile::QOS_PROFILE_DEFAULT;
        let s: heapless::String<32> = qos.to_qos_string();
        assert_eq!(s.as_str(), "::,10:,:,:,,");
    }

    /// issue 0793 — this asserted `1:1:1` (TransientLocal) while testing
    /// `QOS_PROFILE_PARAMETERS`, so it pinned the very defect 0793 fixed: that
    /// profile disagreed with upstream's `rmw_qos_profile_parameters`, which is
    /// VOLATILE. Two tests pinned it, in two crates; correcting the constant and
    /// only the `nros-rmw` test left this one red in tier 2.
    #[test]
    fn test_qos_string_parameters_is_volatile() {
        let qos = QoSProfile::QOS_PROFILE_PARAMETERS;
        let s: heapless::String<32> = qos.to_qos_string();
        assert_eq!(s.as_str(), "::,1000:,:,:,,");
    }

    /// Keeps the TransientLocal ENCODING covered, which the rename above would
    /// otherwise drop: durability 1 vs 2 is the only thing separating these two
    /// strings, and nothing else asserts the 1.
    #[test]
    fn test_qos_string_transient_local() {
        let qos = QoSProfile::QOS_PROFILE_ACTION_STATUS_DEFAULT;
        let s: heapless::String<32> = qos.to_qos_string();
        assert_eq!(s.as_str(), ":1:,1:,:,:,,");
    }

    /// The KEEP_ALL encoding (history digit 2), built rather than borrowed.
    ///
    /// phase-428 W10 — this read `QOS_PROFILE_PARAMETER_EVENTS`, which was
    /// KEEP_ALL only because that preset was WRONG: upstream
    /// `rmw_qos_profile_parameter_events` is KEEP_LAST(1000). So this test both
    /// depended on the defect and helped hide it, and correcting the preset
    /// turned it red — the identical two-crate shape issue 0793 hit four
    /// entries up, which its own comment already describes ("Two tests pinned
    /// it, in two crates").
    ///
    /// The fix is not a new preset to borrow: no NAMED preset is KEEP_ALL now,
    /// and a test about an ENCODING should state the value it encodes.
    #[test]
    fn test_qos_string_keep_all() {
        let qos = QoSProfile::new().keep_all().depth(0);
        let s: heapless::String<32> = qos.to_qos_string();
        assert_eq!(s.as_str(), "::2,0:,:,:,,");
    }

    /// issue 1973 — the decoder inverts the encoder for every profile the
    /// encoder can write, so what our graph walk reports for our own entities
    /// is what they granted.
    #[test]
    fn qos_keyexpr_round_trips_the_encoder() {
        for qos in [
            QoSProfile::QOS_PROFILE_DEFAULT,
            QoSProfile::QOS_PROFILE_SENSOR_DATA,
            QoSProfile::QOS_PROFILE_ACTION_STATUS_DEFAULT,
            QoSProfile::new().keep_all().depth(0),
            QoSProfile::new()
                .best_effort()
                .transient_local()
                .keep_last(42),
        ] {
            let s: heapless::String<32> = qos.to_qos_string();
            let back = qos_from_keyexpr(&s);
            assert_eq!(back.reliability, qos.reliability, "{s}");
            assert_eq!(back.durability, qos.durability, "{s}");
            assert_eq!(back.history, qos.history, "{s}");
            assert_eq!(back.depth, qos.depth, "{s}");
        }
    }

    /// issue 1973 — a stock `rmw_zenoh_cpp` token states the windows and the
    /// liveliness kind; those decode into our milliseconds, and upstream's
    /// RMW_DURATION_INFINITE lands on our infinite sentinel, not on a 49-day
    /// finite window.
    #[test]
    fn qos_keyexpr_decodes_upstream_windows_and_liveliness() {
        let q = qos_from_keyexpr("2:1:1,5:0,500000000:9223372036,854775807:3,1,500");
        assert_eq!(q.reliability, QoSReliabilityPolicy::BestEffort);
        assert_eq!(q.durability, QoSDurabilityPolicy::TransientLocal);
        assert_eq!(q.depth, 5);
        assert_eq!(q.deadline_ms, 500);
        assert_eq!(q.lifespan_ms, DURATION_INFINITE_MS);
        assert_eq!(q.liveliness_kind, QoSLivelinessPolicy::ManualByTopic);
        // 1 s + 500 ns ceils to the next millisecond.
        assert_eq!(q.liveliness_lease_ms, 1001);
    }

    /// issue 1973 — something that is not the grammar reads as "I cannot say",
    /// never as a profile.
    #[test]
    fn qos_keyexpr_malformed_is_unknown() {
        for bad in ["", "1:2", "x:2:1,10:,:,:,,", "1:2:1,10:,:,:,,:extra"] {
            assert_eq!(
                qos_from_keyexpr(bad),
                QoSProfile::QOS_PROFILE_UNKNOWN,
                "{bad:?}"
            );
        }
        let q = qos_from_keyexpr("9:2:1,10:,:,:,,");
        assert_eq!(q.reliability, QoSReliabilityPolicy::Unknown);
    }

    #[test]
    fn test_qos_string_custom() {
        let qos = QoSProfile::new()
            .best_effort()
            .transient_local()
            .keep_all()
            .depth(42);
        let s: heapless::String<32> = qos.to_qos_string();
        assert_eq!(s.as_str(), "2:1:2,:,:,:,,");
    }

    /// phase-428 W5 finding 3 — the namespace never reaches the key, so a name
    /// must arrive here ALREADY RESOLVED.
    ///
    /// `TopicInfo::with_namespace` exists (liveliness tokens use it) and reads
    /// as if it placed the entity in a namespace. `to_key` does not consult
    /// it: the key is `<domain>/<name>/<type>/<hash>`, `name` trimmed of its
    /// slashes and nothing else. So a publisher handed the raw `"chatter"` on a
    /// `/ns` node and a subscription handed the resolved `"/ns/chatter"` are on
    /// two different keys with no error anywhere — which is precisely what
    /// `rclc_publisher_init_with_qos` did until it resolved first.
    ///
    /// This is the CONSEQUENCE half of the fix; the resolution half is pinned
    /// by `nros-c/src/publisher.rs`'s `name_resolution_tests`.
    #[test]
    fn an_unresolved_name_and_its_resolved_form_are_different_keys() {
        const TYPE: &str = "std_msgs::msg::dds_::String_";
        const HASH: &str = "TypeHashNotSupported";

        // What the publisher used to build: the source spelling, with the
        // node's namespace attached beside it.
        let unresolved = TopicInfo::new("chatter", TYPE, HASH)
            .with_domain(0)
            .with_namespace("/ns");
        // What the subscription's registration path builds for the SAME input.
        let resolved = TopicInfo::new("/ns/chatter", TYPE, HASH)
            .with_domain(0)
            .with_namespace("/ns");

        let unresolved_key: heapless::String<128> = unresolved.to_key();
        let resolved_key: heapless::String<128> = resolved.to_key();

        assert_ne!(
            unresolved_key, resolved_key,
            "with_namespace does not place the entity — if these ever match, \
             the key gained a namespace segment and the resolution contract \
             moved"
        );
        assert!(
            unresolved_key.starts_with("0/chatter/"),
            "the namespace is absent from the unresolved key: {unresolved_key}"
        );
        assert!(
            resolved_key.starts_with("0/ns/chatter/"),
            "the resolved name carries the namespace as key segments: {resolved_key}"
        );
    }
}
