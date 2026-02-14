//! DDS name formatting for XRCE-DDS agent.
//!
//! Converts nros-rmw names to DDS-standard topic/type names expected by
//! the Micro-XRCE-DDS Agent.

use heapless::String;

/// Convert an nros topic name to a DDS topic name.
///
/// Strips the leading `/` and prepends `rt/` (ROS topic prefix).
///
/// Example: `"/chatter"` → `"rt/chatter"`
pub fn dds_topic_name<const N: usize>(name: &str) -> String<N> {
    let mut out = String::new();
    let stripped = name.strip_prefix('/').unwrap_or(name);
    let _ = out.push_str("rt/");
    let _ = out.push_str(stripped);
    out
}

/// Convert an nros service name to a DDS request topic name.
///
/// Strips the leading `/` and prepends `rq/`, appends `Request`.
///
/// Example: `"/add_two_ints"` → `"rq/add_two_intsRequest"`
pub fn dds_request_topic<const N: usize>(name: &str) -> String<N> {
    let mut out = String::new();
    let stripped = name.strip_prefix('/').unwrap_or(name);
    let _ = out.push_str("rq/");
    let _ = out.push_str(stripped);
    let _ = out.push_str("Request");
    out
}

/// Convert an nros service name to a DDS reply topic name.
///
/// Strips the leading `/` and prepends `rr/`, appends `Reply`.
///
/// Example: `"/add_two_ints"` → `"rr/add_two_intsReply"`
pub fn dds_reply_topic<const N: usize>(name: &str) -> String<N> {
    let mut out = String::new();
    let stripped = name.strip_prefix('/').unwrap_or(name);
    let _ = out.push_str("rr/");
    let _ = out.push_str(stripped);
    let _ = out.push_str("Reply");
    out
}

/// Convert an nros service type name to a DDS request type name.
///
/// Inserts `Request_` before the trailing `_`.
///
/// Example: `"example_interfaces::srv::dds_::AddTwoInts_"` →
///          `"example_interfaces::srv::dds_::AddTwoInts_Request_"`
pub fn dds_request_type<const N: usize>(type_name: &str) -> String<N> {
    let mut out = String::new();
    if let Some(prefix) = type_name.strip_suffix('_') {
        let _ = out.push_str(prefix);
        let _ = out.push_str("_Request_");
    } else {
        let _ = out.push_str(type_name);
        let _ = out.push_str("_Request_");
    }
    out
}

/// Convert an nros service type name to a DDS reply type name.
///
/// Inserts `Reply_` before the trailing `_`.
///
/// Example: `"example_interfaces::srv::dds_::AddTwoInts_"` →
///          `"example_interfaces::srv::dds_::AddTwoInts_Reply_"`
pub fn dds_reply_type<const N: usize>(type_name: &str) -> String<N> {
    let mut out = String::new();
    if let Some(prefix) = type_name.strip_suffix('_') {
        let _ = out.push_str(prefix);
        let _ = out.push_str("_Reply_");
    } else {
        let _ = out.push_str(type_name);
        let _ = out.push_str("_Reply_");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dds_topic_name() {
        let name: String<64> = dds_topic_name("/chatter");
        assert_eq!(name.as_str(), "rt/chatter");
    }

    #[test]
    fn test_dds_topic_name_no_leading_slash() {
        let name: String<64> = dds_topic_name("chatter");
        assert_eq!(name.as_str(), "rt/chatter");
    }

    #[test]
    fn test_dds_topic_name_nested() {
        let name: String<64> = dds_topic_name("/ns/chatter");
        assert_eq!(name.as_str(), "rt/ns/chatter");
    }

    #[test]
    fn test_dds_request_topic() {
        let name: String<64> = dds_request_topic("/add_two_ints");
        assert_eq!(name.as_str(), "rq/add_two_intsRequest");
    }

    #[test]
    fn test_dds_reply_topic() {
        let name: String<64> = dds_reply_topic("/add_two_ints");
        assert_eq!(name.as_str(), "rr/add_two_intsReply");
    }

    #[test]
    fn test_dds_request_type() {
        let name: String<128> =
            dds_request_type("example_interfaces::srv::dds_::AddTwoInts_");
        assert_eq!(
            name.as_str(),
            "example_interfaces::srv::dds_::AddTwoInts_Request_"
        );
    }

    #[test]
    fn test_dds_reply_type() {
        let name: String<128> =
            dds_reply_type("example_interfaces::srv::dds_::AddTwoInts_");
        assert_eq!(
            name.as_str(),
            "example_interfaces::srv::dds_::AddTwoInts_Reply_"
        );
    }

    #[test]
    fn test_dds_request_type_no_trailing_underscore() {
        let name: String<128> = dds_request_type("MyService");
        assert_eq!(name.as_str(), "MyService_Request_");
    }

    #[test]
    fn test_dds_reply_type_no_trailing_underscore() {
        let name: String<128> = dds_reply_type("MyService");
        assert_eq!(name.as_str(), "MyService_Reply_");
    }
}
