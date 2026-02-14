//! Zenoh key expression generation for RMW types
//!
//! Extension traits that add zenoh-specific key expression methods
//! to the middleware-agnostic `TopicInfo`, `ServiceInfo`, and `QosSettings` types.

use nros_rmw::{
    QosDurabilityPolicy, QosHistoryPolicy, QosReliabilityPolicy, QosSettings, ServiceInfo,
    TopicInfo,
};

/// Extension trait for generating zenoh key expressions from `TopicInfo`
pub trait TopicKeyExpr {
    /// Generate the full topic key in rmw_zenoh format
    /// Format: `<domain_id>/<topic_name>/<type_name>/TypeHashNotSupported`
    fn to_key<const N: usize>(&self) -> heapless::String<N>;

    /// Generate a wildcard topic key for subscribing
    /// Format: `<domain_id>/<topic_name>/<type_name>/*`
    fn to_key_wildcard<const N: usize>(&self) -> heapless::String<N>;
}

impl TopicKeyExpr for TopicInfo<'_> {
    fn to_key<const N: usize>(&self) -> heapless::String<N> {
        let mut key = heapless::String::new();
        let topic_stripped = self.name.trim_matches('/');
        let _ = core::fmt::write(
            &mut key,
            format_args!(
                "{}/{}/{}/TypeHashNotSupported",
                self.domain_id, topic_stripped, self.type_name
            ),
        );
        key
    }

    fn to_key_wildcard<const N: usize>(&self) -> heapless::String<N> {
        let mut key = heapless::String::new();
        let topic_stripped = self.name.trim_matches('/');
        let _ = core::fmt::write(
            &mut key,
            format_args!("{}/{}/{}/*", self.domain_id, topic_stripped, self.type_name),
        );
        key
    }
}

/// Extension trait for generating zenoh key expressions from `ServiceInfo`
pub trait ServiceKeyExpr {
    /// Generate the service key in rmw_zenoh format
    /// Format: `<domain_id>/<service_name>/<type_name>/TypeHashNotSupported`
    fn to_key<const N: usize>(&self) -> heapless::String<N>;

    /// Generate a wildcard service key for client queries
    /// Format: `<domain_id>/<service_name>/<type_name>/*`
    fn to_key_wildcard<const N: usize>(&self) -> heapless::String<N>;
}

impl ServiceKeyExpr for ServiceInfo<'_> {
    fn to_key<const N: usize>(&self) -> heapless::String<N> {
        let mut key = heapless::String::new();
        let service_stripped = self.name.trim_matches('/');
        let _ = core::fmt::write(
            &mut key,
            format_args!(
                "{}/{}/{}/TypeHashNotSupported",
                self.domain_id, service_stripped, self.type_name
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
                self.domain_id, service_stripped, self.type_name
            ),
        );
        key
    }
}

/// Extension trait for generating zenoh QoS liveliness strings from `QosSettings`
pub trait QosKeyExpr {
    /// Convert QoS settings to rmw_zenoh liveliness token format string.
    ///
    /// Format: `reliability:durability:history,depth:deadline:lifespan:liveliness,lease:avoid_ros_namespace_conventions`
    ///
    /// rmw_zenoh encoding:
    /// - reliability: RELIABLE=1, BEST_EFFORT=2
    /// - durability: TRANSIENT_LOCAL=1, VOLATILE=2
    /// - history: KEEP_LAST=1, KEEP_ALL=2
    fn to_qos_string<const N: usize>(&self) -> heapless::String<N>;
}

impl QosKeyExpr for QosSettings {
    fn to_qos_string<const N: usize>(&self) -> heapless::String<N> {
        let mut s = heapless::String::new();

        let reliability = match self.reliability {
            QosReliabilityPolicy::Reliable => 1,
            QosReliabilityPolicy::BestEffort => 2,
        };

        let durability = match self.durability {
            QosDurabilityPolicy::TransientLocal => 1,
            QosDurabilityPolicy::Volatile => 2,
        };

        let history = match self.history {
            QosHistoryPolicy::KeepLast => 1,
            QosHistoryPolicy::KeepAll => 2,
        };

        let _ = core::fmt::write(
            &mut s,
            format_args!(
                "{}:{}:{},{}:,:,:,,",
                reliability, durability, history, self.depth
            ),
        );

        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_topic_key_generation() {
        let topic =
            TopicInfo::new("/chatter", "std_msgs::msg::dds_::String_", "abc123").with_domain(42);
        let key: heapless::String<128> = topic.to_key();
        assert!(key.contains("42"));
        assert!(key.contains("chatter"));
        assert!(key.contains("TypeHashNotSupported"));
    }

    #[test]
    fn test_topic_info_to_key_humble() {
        let topic = TopicInfo {
            name: "/chatter",
            type_name: "std_msgs::msg::dds_::Int32_",
            type_hash: "TypeHashNotSupported",
            domain_id: 0,
        };
        let key: heapless::String<128> = topic.to_key();
        assert_eq!(
            key.as_str(),
            "0/chatter/std_msgs::msg::dds_::Int32_/TypeHashNotSupported"
        );
    }

    #[test]
    fn test_topic_info_to_key_wildcard() {
        let topic = TopicInfo {
            name: "/chatter",
            type_name: "std_msgs::msg::dds_::Int32_",
            type_hash: "TypeHashNotSupported",
            domain_id: 0,
        };
        let key: heapless::String<128> = topic.to_key_wildcard();
        assert_eq!(key.as_str(), "0/chatter/std_msgs::msg::dds_::Int32_/*");
    }

    #[test]
    fn test_service_key() {
        let service = ServiceInfo {
            name: "/add_two_ints",
            type_name: "example_interfaces::srv::dds_::AddTwoInts",
            type_hash: "TypeHashNotSupported",
            domain_id: 0,
        };
        let key: heapless::String<128> = service.to_key();
        assert!(key.contains("add_two_ints"));
        assert!(key.contains("TypeHashNotSupported"));
    }

    #[test]
    fn test_qos_string_sensor_data() {
        let qos = QosSettings::QOS_PROFILE_SENSOR_DATA;
        let s: heapless::String<32> = qos.to_qos_string();
        assert_eq!(s.as_str(), "2:2:1,5:,:,:,,");
    }

    #[test]
    fn test_qos_string_default() {
        let qos = QosSettings::QOS_PROFILE_DEFAULT;
        let s: heapless::String<32> = qos.to_qos_string();
        assert_eq!(s.as_str(), "1:2:1,10:,:,:,,");
    }

    #[test]
    fn test_qos_string_transient_local() {
        let qos = QosSettings::QOS_PROFILE_PARAMETERS;
        let s: heapless::String<32> = qos.to_qos_string();
        assert_eq!(s.as_str(), "1:1:1,1000:,:,:,,");
    }

    #[test]
    fn test_qos_string_keep_all() {
        let qos = QosSettings::QOS_PROFILE_PARAMETER_EVENTS;
        let s: heapless::String<32> = qos.to_qos_string();
        assert_eq!(s.as_str(), "1:2:2,0:,:,:,,");
    }

    #[test]
    fn test_qos_string_custom() {
        let qos = QosSettings::new()
            .best_effort()
            .transient_local()
            .keep_all()
            .depth(42);
        let s: heapless::String<32> = qos.to_qos_string();
        assert_eq!(s.as_str(), "2:1:2,42:,:,:,,");
    }
}
