mod action;
mod common;
pub mod cpp;
mod msg;
mod srv;

pub use action::{
    GeneratedActionPackage, GeneratedCActionPackage, GeneratedNrosActionPackage,
    generate_action_package, generate_c_action_package, generate_nros_action_package,
    generate_nros_inline_action,
};
pub use common::GeneratorError;
pub use cpp::{
    GeneratedCppActionPackage, GeneratedCppPackage, GeneratedCppServicePackage,
    generate_cpp_action_package, generate_cpp_message_package, generate_cpp_service_package,
};
pub use msg::{
    GeneratedCPackage, GeneratedNrosPackage, GeneratedPackage, generate_c_message_package,
    generate_message_package, generate_nros_inline_message, generate_nros_message_package,
};
pub use srv::{
    GeneratedCServicePackage, GeneratedNrosServicePackage, GeneratedServicePackage,
    generate_c_service_package, generate_nros_inline_service, generate_nros_service_package,
    generate_service_package,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::RosEdition;
    use rosidl_parser::{
        Field, FieldType, PrimitiveType, parse_action, parse_message, parse_service,
    };
    use std::collections::HashSet;

    #[test]
    fn test_simple_message_generation() {
        let msg = parse_message("int32 x\nfloat64 y\n").unwrap();
        let deps = HashSet::new();

        let result = generate_message_package("test_msgs", "Point", &msg, &deps);
        assert!(result.is_ok());

        let pkg = result.unwrap();
        assert!(pkg.cargo_toml.contains("test_msgs"));
        assert!(pkg.message_rmw.contains("i32"));
        assert!(pkg.message_rmw.contains("f64"));
    }

    #[test]
    fn test_message_with_dependencies() {
        let msg = parse_message("geometry_msgs/Point position\n").unwrap();
        let deps = HashSet::new();

        let result = generate_message_package("nav_msgs", "Odometry", &msg, &deps);
        assert!(result.is_ok());

        let pkg = result.unwrap();
        assert!(pkg.cargo_toml.contains("geometry_msgs"));
    }

    #[test]
    fn test_message_with_large_array() {
        let mut msg = rosidl_parser::Message::new();
        msg.fields.push(Field {
            field_type: FieldType::Array {
                element_type: Box::new(FieldType::Primitive(PrimitiveType::Int32)),
                size: 64,
            },
            name: "data".to_string(),
            default_value: None,
        });

        let deps = HashSet::new();
        let result = generate_message_package("test_msgs", "LargeArray", &msg, &deps);
        assert!(result.is_ok());

        let pkg = result.unwrap();
        assert!(pkg.cargo_toml.contains("big-array"));
    }

    #[test]
    fn test_message_with_keyword_field() {
        let msg = parse_message("int32 type\nfloat64 match\n").unwrap();
        let deps = HashSet::new();

        let result = generate_message_package("test_msgs", "Keywords", &msg, &deps);
        assert!(result.is_ok());

        let pkg = result.unwrap();
        assert!(pkg.message_rmw.contains("type_"));
        assert!(pkg.message_rmw.contains("match_"));
    }

    #[test]
    fn test_simple_service_generation() {
        let srv = parse_service("int32 a\nint32 b\n---\nint32 sum\n").unwrap();
        let deps = HashSet::new();

        let result = generate_service_package("example_interfaces", "AddTwoInts", &srv, &deps);
        assert!(result.is_ok());

        let pkg = result.unwrap();
        assert!(pkg.cargo_toml.contains("example_interfaces"));
        assert!(pkg.lib_rs.contains("pub mod srv"));
        assert!(pkg.service_rmw.contains("AddTwoIntsRequest"));
        assert!(pkg.service_rmw.contains("AddTwoIntsResponse"));
        assert!(pkg.service_idiomatic.contains("AddTwoIntsRequest"));
        assert!(pkg.service_idiomatic.contains("AddTwoIntsResponse"));
    }

    #[test]
    fn test_service_with_dependencies() {
        let srv = parse_service("geometry_msgs/Point position\n---\nbool success\n").unwrap();
        let deps = HashSet::new();

        let result = generate_service_package("test_srvs", "CheckPoint", &srv, &deps);
        assert!(result.is_ok());

        let pkg = result.unwrap();
        assert!(pkg.cargo_toml.contains("geometry_msgs"));
    }

    #[test]
    fn test_simple_action_generation() {
        let action =
            parse_action("int32 order\n---\nint32[] sequence\n---\nint32[] partial_sequence\n")
                .unwrap();
        let deps = HashSet::new();

        let result = generate_action_package("example_interfaces", "Fibonacci", &action, &deps);
        assert!(result.is_ok());

        let pkg = result.unwrap();
        assert!(pkg.cargo_toml.contains("example_interfaces"));
        assert!(pkg.lib_rs.contains("pub mod action"));
        assert!(pkg.action_rmw.contains("FibonacciGoal"));
        assert!(pkg.action_rmw.contains("FibonacciResult"));
        assert!(pkg.action_rmw.contains("FibonacciFeedback"));
        assert!(pkg.action_idiomatic.contains("FibonacciGoal"));
        assert!(pkg.action_idiomatic.contains("FibonacciResult"));
        assert!(pkg.action_idiomatic.contains("FibonacciFeedback"));
    }

    #[test]
    fn test_action_with_dependencies() {
        let action = parse_action(
            "geometry_msgs/Point target\n---\nfloat64 distance\n---\nfloat64 current_distance\n",
        )
        .unwrap();
        let deps = HashSet::new();

        let result = generate_action_package("test_actions", "Navigate", &action, &deps);
        assert!(result.is_ok());

        let pkg = result.unwrap();
        assert!(pkg.cargo_toml.contains("geometry_msgs"));
    }

    // ========================================================================
    // nros Backend Tests
    // ========================================================================

    #[test]
    fn test_nros_simple_message_generation() {
        let msg = parse_message("int32 x\nfloat64 y\nstring name\n").unwrap();
        let deps = HashSet::new();

        let result = generate_nros_message_package(
            "test_msgs",
            "Point",
            &msg,
            &deps,
            "0.1.0",
            RosEdition::Humble,
            &crate::config::CapacityResolver::empty(),
        );
        assert!(result.is_ok());

        let pkg = result.unwrap();

        // Check Cargo.toml has nros dependencies
        assert!(pkg.cargo_toml.contains("nros-core"));
        assert!(pkg.cargo_toml.contains("nros-serdes"));
        assert!(pkg.cargo_toml.contains("heapless"));

        // Check lib.rs is no_std
        assert!(pkg.lib_rs.contains("#![no_std]"));
        assert!(pkg.lib_rs.contains("pub mod msg"));

        // Check message contains proper types
        assert!(pkg.message_rs.contains("pub x: i32"));
        assert!(pkg.message_rs.contains("pub y: f64"));
        assert!(pkg.message_rs.contains("heapless::String<256>"));

        // Check it has Serialize/Deserialize implementations
        assert!(pkg.message_rs.contains("impl Serialize for Point"));
        assert!(pkg.message_rs.contains("impl Deserialize for Point"));
        assert!(pkg.message_rs.contains("impl RosMessage for Point"));
    }

    #[test]
    fn test_nros_message_with_sequence() {
        let msg = parse_message("int32[] data\n").unwrap();
        let deps = HashSet::new();

        let result = generate_nros_message_package(
            "test_msgs",
            "IntArray",
            &msg,
            &deps,
            "0.1.0",
            RosEdition::Humble,
            &crate::config::CapacityResolver::empty(),
        );
        assert!(result.is_ok());

        let pkg = result.unwrap();
        // Check sequence uses heapless::Vec
        assert!(pkg.message_rs.contains("heapless::Vec<i32"));
    }

    #[test]
    fn test_nros_service_generation() {
        let srv = parse_service("int64 a\nint64 b\n---\nint64 sum\n").unwrap();
        let deps = HashSet::new();

        let result = generate_nros_service_package(
            "test_srvs",
            "AddTwoInts",
            &srv,
            &deps,
            "0.1.0",
            RosEdition::Humble,
        );
        assert!(result.is_ok());

        let pkg = result.unwrap();

        // Check Cargo.toml
        assert!(pkg.cargo_toml.contains("nros-core"));

        // Check lib.rs
        assert!(pkg.lib_rs.contains("pub mod srv"));

        // Check service types
        assert!(pkg.service_rs.contains("AddTwoIntsRequest"));
        assert!(pkg.service_rs.contains("AddTwoIntsResponse"));
        assert!(pkg.service_rs.contains("pub a: i64"));
        assert!(pkg.service_rs.contains("pub b: i64"));
        assert!(pkg.service_rs.contains("pub sum: i64"));

        // Check RosService impl
        assert!(pkg.service_rs.contains("impl RosService for AddTwoInts"));

        // K.7.1.c: nros_serdes::Message impl on both halves with the
        // rosidl `<pkg>/srv/<Svc>_Request|Response` TYPE_NAME convention.
        assert!(
            pkg.service_rs
                .contains("impl ::nros_serdes::Message for AddTwoIntsRequest")
        );
        assert!(
            pkg.service_rs
                .contains("impl ::nros_serdes::Message for AddTwoIntsResponse")
        );
        assert!(
            pkg.service_rs
                .contains("\"test_srvs/srv/AddTwoInts_Request\"")
        );
        assert!(
            pkg.service_rs
                .contains("\"test_srvs/srv/AddTwoInts_Response\"")
        );
    }

    #[test]
    fn test_nros_action_generation() {
        let action =
            parse_action("int32 order\n---\nint32[] sequence\n---\nint32[] partial_sequence\n")
                .unwrap();
        let deps = HashSet::new();

        let result = generate_nros_action_package(
            "example_interfaces",
            "Fibonacci",
            &action,
            &deps,
            "0.1.0",
            RosEdition::Humble,
        );
        assert!(result.is_ok());

        let pkg = result.unwrap();

        // Check Cargo.toml
        assert!(pkg.cargo_toml.contains("nros-core"));

        // Check lib.rs
        assert!(pkg.lib_rs.contains("pub mod action"));

        // Check action types
        assert!(pkg.action_rs.contains("FibonacciGoal"));
        assert!(pkg.action_rs.contains("FibonacciResult"));
        assert!(pkg.action_rs.contains("FibonacciFeedback"));
        assert!(pkg.action_rs.contains("pub order: i32"));

        // Check RosAction impl
        assert!(pkg.action_rs.contains("impl RosAction for Fibonacci"));
        assert!(pkg.action_rs.contains("type Goal = FibonacciGoal"));
        assert!(pkg.action_rs.contains("type Result = FibonacciResult"));
        assert!(pkg.action_rs.contains("type Feedback = FibonacciFeedback"));

        // K.7.1.c: nros_serdes::Message impl on all three halves with
        // the rosidl `<pkg>/action/<Action>_<Half>` TYPE_NAME convention.
        assert!(
            pkg.action_rs
                .contains("impl ::nros_serdes::Message for FibonacciGoal")
        );
        assert!(
            pkg.action_rs
                .contains("impl ::nros_serdes::Message for FibonacciResult")
        );
        assert!(
            pkg.action_rs
                .contains("impl ::nros_serdes::Message for FibonacciFeedback")
        );
        assert!(
            pkg.action_rs
                .contains("\"example_interfaces/action/Fibonacci_Goal\"")
        );
        assert!(
            pkg.action_rs
                .contains("\"example_interfaces/action/Fibonacci_Result\"")
        );
        assert!(
            pkg.action_rs
                .contains("\"example_interfaces/action/Fibonacci_Feedback\"")
        );
        // Sequence-of-int32 for Result + Feedback emits prefixed FT_ idents
        // to avoid module-scope collisions.
        assert!(pkg.action_rs.contains("RESULT_FT_SEQUENCE_ELEM"));
        assert!(pkg.action_rs.contains("FEEDBACK_FT_PARTIAL_SEQUENCE_ELEM"));

        // ----------------------------------------------------------------
        // K.7.1.d — five rosidl envelope structs each with
        // Serialize / Deserialize / RosMessage / ::nros_serdes::Message.
        // ----------------------------------------------------------------
        for envelope in &[
            "Fibonacci_SendGoal_Request",
            "Fibonacci_SendGoal_Response",
            "Fibonacci_GetResult_Request",
            "Fibonacci_GetResult_Response",
            "Fibonacci_FeedbackMessage",
        ] {
            assert!(
                pkg.action_rs.contains(&format!("pub struct {}", envelope)),
                "missing struct {}",
                envelope
            );
            assert!(
                pkg.action_rs
                    .contains(&format!("impl Serialize for {}", envelope)),
                "missing Serialize for {}",
                envelope
            );
            assert!(
                pkg.action_rs
                    .contains(&format!("impl Deserialize for {}", envelope)),
                "missing Deserialize for {}",
                envelope
            );
            assert!(
                pkg.action_rs
                    .contains(&format!("impl RosMessage for {}", envelope)),
                "missing RosMessage for {}",
                envelope
            );
            assert!(
                pkg.action_rs
                    .contains(&format!("impl ::nros_serdes::Message for {}", envelope)),
                "missing nros_serdes::Message for {}",
                envelope
            );
        }
        // Envelope field layout: goal_id is the nested UUID, NOT a flat [u8;16].
        assert!(
            pkg.action_rs
                .contains("pub goal_id: unique_identifier_msgs::msg::UUID")
        );
        assert!(pkg.action_rs.contains("pub goal: FibonacciGoal"));
        assert!(pkg.action_rs.contains("pub accepted: bool"));
        assert!(
            pkg.action_rs
                .contains("pub stamp: builtin_interfaces::msg::Time")
        );
        assert!(pkg.action_rs.contains("pub status: i8"));
        assert!(pkg.action_rs.contains("pub result: FibonacciResult"));
        assert!(pkg.action_rs.contains("pub feedback: FibonacciFeedback"));
        // ROS-form Message::TYPE_NAME on each envelope.
        assert!(
            pkg.action_rs
                .contains("\"example_interfaces/action/Fibonacci_SendGoal_Request\"")
        );
        assert!(
            pkg.action_rs
                .contains("\"example_interfaces/action/Fibonacci_SendGoal_Response\"")
        );
        assert!(
            pkg.action_rs
                .contains("\"example_interfaces/action/Fibonacci_GetResult_Request\"")
        );
        assert!(
            pkg.action_rs
                .contains("\"example_interfaces/action/Fibonacci_GetResult_Response\"")
        );
        assert!(
            pkg.action_rs
                .contains("\"example_interfaces/action/Fibonacci_FeedbackMessage\"")
        );
        // DDS-mangled RosMessage::TYPE_NAME (one spot-check).
        assert!(
            pkg.action_rs
                .contains("\"example_interfaces::action::dds_::Fibonacci_SendGoal_Request_\"")
        );
        // K.7.1.d: action Cargo.toml depends on unique_identifier_msgs
        // + builtin_interfaces for the envelope nested types.
        assert!(pkg.cargo_toml.contains("unique_identifier_msgs"));
        assert!(pkg.cargo_toml.contains("builtin_interfaces"));

        // ----------------------------------------------------------------
        // K.7.1.d.b — RosAction impl binds each of the five envelope
        // structs as an associated type so generic
        // `register_type::<A::SendGoalRequest>()` calls in nano-ros's
        // runtime can name them without knowing the action's concrete
        // module path.
        // ----------------------------------------------------------------
        assert!(
            pkg.action_rs
                .contains("type SendGoalRequest = Fibonacci_SendGoal_Request"),
            "missing associated type SendGoalRequest"
        );
        assert!(
            pkg.action_rs
                .contains("type SendGoalResponse = Fibonacci_SendGoal_Response"),
            "missing associated type SendGoalResponse"
        );
        assert!(
            pkg.action_rs
                .contains("type GetResultRequest = Fibonacci_GetResult_Request"),
            "missing associated type GetResultRequest"
        );
        assert!(
            pkg.action_rs
                .contains("type GetResultResponse = Fibonacci_GetResult_Response"),
            "missing associated type GetResultResponse"
        );
        assert!(
            pkg.action_rs
                .contains("type FeedbackMessage = Fibonacci_FeedbackMessage"),
            "missing associated type FeedbackMessage"
        );
    }

    // ========================================================================
    // C Code Generation Tests
    // ========================================================================

    #[test]
    fn test_c_simple_message_generation() {
        let msg = parse_message("int32 x\nfloat64 y\nbool flag\n").unwrap();
        let type_hash = "abc123";

        let result = generate_c_message_package(
            "test_msgs",
            "Point",
            &msg,
            type_hash,
            &crate::config::CapacityResolver::empty(),
        );
        assert!(result.is_ok());

        let pkg = result.unwrap();

        // Check header file
        assert!(pkg.header.contains("#ifndef TEST_MSGS_MSG_POINT_H"));
        assert!(pkg.header.contains("typedef struct test_msgs_msg_point"));
        assert!(pkg.header.contains("int32_t x"));
        assert!(pkg.header.contains("double y"));
        assert!(pkg.header.contains("bool flag"));
        assert!(pkg.header.contains("test_msgs_msg_point_init"));
        assert!(pkg.header.contains("test_msgs_msg_point_serialize"));
        assert!(pkg.header.contains("test_msgs_msg_point_deserialize"));

        // Check source file
        assert!(pkg.source.contains("test_msgs_msg_point.h"));
        assert!(pkg.source.contains("nros_cdr_write_i32"));
        assert!(pkg.source.contains("nros_cdr_write_f64"));
        assert!(pkg.source.contains("nros_cdr_write_bool"));

        // Check file names
        assert_eq!(pkg.header_name, "test_msgs_msg_point.h");
        assert_eq!(pkg.source_name, "test_msgs_msg_point.c");
    }

    #[test]
    fn test_c_message_with_string() {
        let msg = parse_message("string name\n").unwrap();
        let type_hash = "def456";

        let result = generate_c_message_package(
            "std_msgs",
            "String",
            &msg,
            type_hash,
            &crate::config::CapacityResolver::empty(),
        );
        assert!(result.is_ok());

        let pkg = result.unwrap();
        assert!(pkg.header.contains("char name[256]"));
        assert!(pkg.source.contains("nros_cdr_write_string"));
    }

    #[test]
    fn test_c_message_with_array() {
        let msg = parse_message("int32[3] values\n").unwrap();
        let type_hash = "ghi789";

        let result = generate_c_message_package(
            "test_msgs",
            "IntArray",
            &msg,
            type_hash,
            &crate::config::CapacityResolver::empty(),
        );
        assert!(result.is_ok());

        let pkg = result.unwrap();
        assert!(pkg.header.contains("int32_t values[3]"));
        assert!(pkg.source.contains("for (size_t i = 0; i < 3; ++i)"));
    }

    #[test]
    fn test_c_per_field_capacity_config() {
        // RFC-0033: big unbounded sequence + small unbounded string, one message.
        let msg = parse_message("uint8[] pixels\nstring label\n").unwrap();
        let resolver = crate::config::CapacityResolver::from_toml_str(
            r#"
            [fields]
            "my_msgs/Frame.pixels" = 921600
            "my_msgs/Frame.label"  = 16
            "#,
        )
        .unwrap();
        let pkg = generate_c_message_package("my_msgs", "Frame", &msg, "h", &resolver).unwrap();
        // Sequence capacity reflected in both the inline struct and the count.
        assert!(
            pkg.header.contains("data[921600]"),
            "seq cap missing:\n{}",
            pkg.header
        );
        // Small string char buffer.
        assert!(
            pkg.header.contains("char label[16]"),
            "string cap missing:\n{}",
            pkg.header
        );
        assert!(!pkg.header.contains("[64]"));
    }

    #[test]
    fn test_c_borrowed_mode_errors() {
        let msg = parse_message("uint8[] data\n").unwrap();
        let resolver = crate::config::CapacityResolver::from_toml_str(
            r#"
            [fields]
            "my_msgs/Blob.data" = { cap = 100, mode = "borrowed" }
            "#,
        )
        .unwrap();
        let err = match generate_c_message_package("my_msgs", "Blob", &msg, "h", &resolver) {
            Ok(_) => panic!("expected unsupported-mode error"),
            Err(e) => e.to_string(),
        };
        assert!(err.contains("borrowed"), "{err}");
        assert!(err.contains("not yet supported"), "{err}");
    }

    #[test]
    fn test_c_heap_primitive_sequence() {
        // RFC-0033 mode = "heap" → rclc-style `{ T* data; size_t size, capacity; }`.
        let msg = parse_message("uint8[] data\n").unwrap();
        let resolver = crate::config::CapacityResolver::from_toml_str(
            r#"
            [fields]
            "my_msgs/Blob.data" = { cap = 0, mode = "heap" }
            "#,
        )
        .unwrap();
        let pkg = generate_c_message_package("my_msgs", "Blob", &msg, "h", &resolver).unwrap();
        // Heap struct in the header (pointer + size + capacity, no inline array).
        assert!(
            pkg.header
                .contains("struct { uint8_t* data; size_t size; size_t capacity; } data"),
            "heap struct missing:\n{}",
            pkg.header
        );
        assert!(pkg.header.contains("_fini"), "fini decl missing");
        // Deserialize mallocs via the platform allocator; fini frees.
        assert!(
            pkg.source.contains("nros_platform_malloc"),
            "{}",
            pkg.source
        );
        assert!(pkg.source.contains("nros_platform_free"), "{}", pkg.source);
        assert!(!pkg.header.contains("data[64]"));
    }

    #[test]
    fn test_c_heap_string() {
        // RFC-0033 mode = "heap" on a string → rclc rosidl_runtime_c__String shape.
        let msg = parse_message("string label\n").unwrap();
        let resolver = crate::config::CapacityResolver::from_toml_str(
            r#"
            [fields]
            "my_msgs/M.label" = { cap = 0, mode = "heap" }
            "#,
        )
        .unwrap();
        let pkg = generate_c_message_package("my_msgs", "M", &msg, "h", &resolver).unwrap();
        assert!(
            pkg.header
                .contains("struct { char* data; size_t size; size_t capacity; } label"),
            "heap string struct missing:\n{}",
            pkg.header
        );
        assert!(
            pkg.source.contains("nros_platform_malloc"),
            "{}",
            pkg.source
        );
        assert!(pkg.source.contains("memcpy"), "{}", pkg.source);
        assert!(!pkg.header.contains("char label[256]"));
    }

    #[test]
    fn test_c_heap_seq_of_strings_unsupported() {
        // Heap sequences of strings / nested messages remain unsupported in C.
        let msg = parse_message("string[] tags\n").unwrap();
        let resolver = crate::config::CapacityResolver::from_toml_str(
            r#"
            [fields]
            "my_msgs/M.tags" = { cap = 0, mode = "heap" }
            "#,
        )
        .unwrap();
        let err = match generate_c_message_package("my_msgs", "M", &msg, "h", &resolver) {
            Ok(_) => panic!("expected unsupported-mode error"),
            Err(e) => e.to_string(),
        };
        assert!(
            err.contains("heap") && err.contains("not yet supported"),
            "{err}"
        );
    }

    #[test]
    fn test_c_simple_service_generation() {
        let srv = parse_service("int32 a\nint32 b\n---\nint32 sum\n").unwrap();
        let type_hash = "srv123";

        let result = generate_c_service_package("test_srvs", "AddTwoInts", &srv, type_hash);
        assert!(result.is_ok());

        let pkg = result.unwrap();

        // Check header file
        assert!(pkg.header.contains("#ifndef TEST_SRVS_SRV_ADD_TWO_INTS_H"));
        assert!(
            pkg.header
                .contains("typedef struct test_srvs_srv_add_two_ints_request")
        );
        assert!(
            pkg.header
                .contains("typedef struct test_srvs_srv_add_two_ints_response")
        );
        assert!(pkg.header.contains("int32_t a"));
        assert!(pkg.header.contains("int32_t b"));
        assert!(pkg.header.contains("int32_t sum"));

        // Check source file
        assert!(
            pkg.source
                .contains("test_srvs_srv_add_two_ints_request_init")
        );
        assert!(
            pkg.source
                .contains("test_srvs_srv_add_two_ints_response_init")
        );
        assert!(
            pkg.source
                .contains("test_srvs_srv_add_two_ints_request_serialize")
        );
        assert!(
            pkg.source
                .contains("test_srvs_srv_add_two_ints_response_serialize")
        );

        // Check file names
        assert_eq!(pkg.header_name, "test_srvs_srv_add_two_ints.h");
        assert_eq!(pkg.source_name, "test_srvs_srv_add_two_ints.c");
    }

    #[test]
    fn test_c_simple_action_generation() {
        let action =
            parse_action("int32 order\n---\nint32 result_code\n---\nint32 progress\n").unwrap();
        let type_hash = "act456";

        let result = generate_c_action_package("test_actions", "Fibonacci", &action, type_hash);
        assert!(result.is_ok());

        let pkg = result.unwrap();

        // Check header file
        assert!(
            pkg.header
                .contains("#ifndef TEST_ACTIONS_ACTION_FIBONACCI_H")
        );
        assert!(
            pkg.header
                .contains("typedef struct test_actions_action_fibonacci_goal")
        );
        assert!(
            pkg.header
                .contains("typedef struct test_actions_action_fibonacci_result")
        );
        assert!(
            pkg.header
                .contains("typedef struct test_actions_action_fibonacci_feedback")
        );
        assert!(pkg.header.contains("int32_t order"));
        assert!(pkg.header.contains("int32_t result_code"));
        assert!(pkg.header.contains("int32_t progress"));

        // Check source file
        assert!(
            pkg.source
                .contains("test_actions_action_fibonacci_goal_init")
        );
        assert!(
            pkg.source
                .contains("test_actions_action_fibonacci_result_init")
        );
        assert!(
            pkg.source
                .contains("test_actions_action_fibonacci_feedback_init")
        );

        // Check file names
        assert_eq!(pkg.header_name, "test_actions_action_fibonacci.h");
        assert_eq!(pkg.source_name, "test_actions_action_fibonacci.c");
    }

    // ========================================================================
    // C++ Code Generation Tests
    // ========================================================================

    #[test]
    fn test_cpp_simple_message_generation() {
        let msg = parse_message("int32 data\n").unwrap();
        let type_hash = "TypeHashNotSupported";

        let result = generate_cpp_message_package(
            "std_msgs",
            "Int32",
            &msg,
            type_hash,
            &crate::config::CapacityResolver::empty(),
        );
        assert!(result.is_ok());

        let pkg = result.unwrap();

        // Check header
        assert!(pkg.header.contains("#ifndef STD_MSGS_MSG_INT32_HPP"));
        assert!(pkg.header.contains("namespace std_msgs { namespace msg {"));
        assert!(pkg.header.contains("struct Int32"));
        assert!(pkg.header.contains("int32_t data"));
        assert!(pkg.header.contains("TYPE_NAME"));
        assert!(pkg.header.contains("TYPE_HASH"));
        assert!(pkg.header.contains("SERIALIZED_SIZE_MAX"));
        assert!(pkg.header.contains("ffi_publish"));
        assert!(pkg.header.contains("ffi_deserialize"));

        // Check FFI Rust
        assert!(pkg.ffi_rs.contains("#[repr(C)]"));
        assert!(pkg.ffi_rs.contains("std_msgs_msg_int32_t"));
        assert!(pkg.ffi_rs.contains("write_i32"));
        assert!(pkg.ffi_rs.contains("nros_cpp_publish_std_msgs_msg_int32"));
        assert!(
            pkg.ffi_rs
                .contains("nros_cpp_deserialize_std_msgs_msg_int32")
        );

        // Check filenames
        assert_eq!(pkg.header_name, "std_msgs_msg_int32.hpp");
        assert_eq!(pkg.ffi_rs_name, "std_msgs_msg_int32_ffi.rs");
    }

    #[test]
    fn test_cpp_message_with_string() {
        let msg = parse_message("string data\n").unwrap();
        let type_hash = "TypeHashNotSupported";

        let result = generate_cpp_message_package(
            "std_msgs",
            "String",
            &msg,
            type_hash,
            &crate::config::CapacityResolver::empty(),
        );
        assert!(result.is_ok());

        let pkg = result.unwrap();

        // C++ header should use FixedString
        assert!(pkg.header.contains("nros::FixedString<256>"));
        assert!(pkg.header.contains("fixed_string.hpp"));

        // Rust FFI should use [u8; 256] and write_string
        assert!(pkg.ffi_rs.contains("[u8; 256]"));
        assert!(pkg.ffi_rs.contains("write_string"));
    }

    #[test]
    fn test_cpp_message_with_array() {
        let msg = parse_message("int32[3] values\n").unwrap();
        let type_hash = "TypeHashNotSupported";

        let result = generate_cpp_message_package(
            "test_msgs",
            "IntArray",
            &msg,
            type_hash,
            &crate::config::CapacityResolver::empty(),
        );
        assert!(result.is_ok());

        let pkg = result.unwrap();

        // C++ header: int32_t values[3]
        assert!(pkg.header.contains("int32_t"));
        assert!(pkg.header.contains("[3]"));

        // Rust FFI: [i32; 3] and loop with write_i32
        assert!(pkg.ffi_rs.contains("[i32; 3]"));
        assert!(pkg.ffi_rs.contains("for i in 0..3"));
    }

    #[test]
    fn test_cpp_message_with_sequence() {
        let msg = parse_message("int32[] data\n").unwrap();
        let type_hash = "TypeHashNotSupported";

        let result = generate_cpp_message_package(
            "test_msgs",
            "IntSeq",
            &msg,
            type_hash,
            &crate::config::CapacityResolver::empty(),
        );
        assert!(result.is_ok());

        let pkg = result.unwrap();

        // C++ header: FixedSequence
        assert!(pkg.header.contains("nros::FixedSequence<int32_t, 64>"));

        // Rust FFI: sequence struct with size + data
        assert!(pkg.ffi_rs.contains("_seq_t"));
        assert!(pkg.ffi_rs.contains("pub size: u32"));
        assert!(pkg.ffi_rs.contains("write_u32"));
    }

    #[test]
    fn test_cpp_per_field_capacity_config() {
        // RFC-0033: header type AND FFI repr must both reflect the resolved cap.
        let msg = parse_message("uint8[] pixels\nstring label\n").unwrap();
        let resolver = crate::config::CapacityResolver::from_toml_str(
            r#"
            [fields]
            "my_msgs/Frame.pixels" = 921600
            "my_msgs/Frame.label"  = 16
            "#,
        )
        .unwrap();
        let pkg = generate_cpp_message_package("my_msgs", "Frame", &msg, "h", &resolver).unwrap();

        // Header: configured sequence + string capacities.
        assert!(
            pkg.header.contains("nros::FixedSequence<uint8_t, 921600>"),
            "header seq cap:\n{}",
            pkg.header
        );
        assert!(
            pkg.header.contains("nros::FixedString<16>"),
            "header string cap:\n{}",
            pkg.header
        );
        // FFI repr must agree: [u8; 16] string + sequence struct capacity 921600.
        assert!(
            pkg.ffi_rs.contains("[u8; 16]"),
            "ffi string repr:\n{}",
            pkg.ffi_rs
        );
        assert!(
            pkg.ffi_rs.contains("921600"),
            "ffi seq cap:\n{}",
            pkg.ffi_rs
        );
        assert!(!pkg.header.contains(", 64>"));
        assert!(!pkg.ffi_rs.contains("[u8; 256]"));
    }

    #[test]
    fn test_cpp_heap_string() {
        // RFC-0033 mode = "heap" on a string → nros::HeapString + nros_cpp_heap_str_t FFI repr.
        let msg = parse_message("string label\n").unwrap();
        let resolver = crate::config::CapacityResolver::from_toml_str(
            r#"
            [fields]
            "my_msgs/M.label" = { cap = 0, mode = "heap" }
            "#,
        )
        .unwrap();
        let pkg = generate_cpp_message_package("my_msgs", "M", &msg, "h", &resolver).unwrap();
        assert!(
            pkg.header.contains("nros::HeapString"),
            "header heap string:\n{}",
            pkg.header
        );
        assert!(
            pkg.header.contains("heap_string.hpp"),
            "heap string include missing"
        );
        assert!(
            pkg.ffi_rs.contains("nros_cpp_heap_str_t"),
            "ffi heap string repr:\n{}",
            pkg.ffi_rs
        );
        assert!(
            pkg.ffi_rs.contains("nros_platform_malloc"),
            "{}",
            pkg.ffi_rs
        );
        assert!(!pkg.ffi_rs.contains("[u8; 256]"));
    }

    #[test]
    fn test_cpp_heap_seq_of_strings_unsupported() {
        let msg = parse_message("string[] tags\n").unwrap();
        let resolver = crate::config::CapacityResolver::from_toml_str(
            r#"
            [fields]
            "my_msgs/M.tags" = { cap = 0, mode = "heap" }
            "#,
        )
        .unwrap();
        let err = match generate_cpp_message_package("my_msgs", "M", &msg, "h", &resolver) {
            Ok(_) => panic!("expected unsupported-mode error"),
            Err(e) => e.to_string(),
        };
        assert!(
            err.contains("heap") && err.contains("not yet supported"),
            "{err}"
        );
    }

    #[test]
    fn test_cpp_heap_primitive_sequence() {
        // RFC-0033 mode = "heap" → nros::HeapSequence<T> header + pointer-trio FFI repr.
        let msg = parse_message("uint8[] pixels\nint32 seq\n").unwrap();
        let resolver = crate::config::CapacityResolver::from_toml_str(
            r#"
            [fields]
            "my_msgs/Frame.pixels" = { cap = 0, mode = "heap" }
            "#,
        )
        .unwrap();
        let pkg = generate_cpp_message_package("my_msgs", "Frame", &msg, "h", &resolver).unwrap();
        // Header: HeapSequence type + include.
        assert!(
            pkg.header.contains("nros::HeapSequence<uint8_t>"),
            "header heap type:\n{}",
            pkg.header
        );
        assert!(
            pkg.header.contains("heap_sequence.hpp"),
            "heap include missing"
        );
        // FFI: pointer-trio repr + shared allocator + heap publish buffer.
        assert!(
            pkg.ffi_rs.contains("pub data: *mut u8"),
            "ffi heap repr:\n{}",
            pkg.ffi_rs
        );
        assert!(
            pkg.ffi_rs.contains("nros_platform_malloc"),
            "{}",
            pkg.ffi_rs
        );
        assert!(pkg.ffi_rs.contains("nros_platform_free"), "{}", pkg.ffi_rs);
        // No fixed inline array for the heap field.
        assert!(!pkg.ffi_rs.contains("data: [u8; 64]"));
    }

    #[test]
    fn test_cpp_simple_service_generation() {
        let srv = parse_service("int32 a\nint32 b\n---\nint32 sum\n").unwrap();
        let type_hash = "TypeHashNotSupported";

        let result = generate_cpp_service_package("test_srvs", "AddTwoInts", &srv, type_hash);
        assert!(result.is_ok());

        let pkg = result.unwrap();

        // Header checks
        assert!(
            pkg.header
                .contains("#ifndef TEST_SRVS_SRV_ADD_TWO_INTS_HPP")
        );
        assert!(pkg.header.contains("namespace test_srvs { namespace srv {"));
        assert!(pkg.header.contains("struct AddTwoInts"));
        assert!(pkg.header.contains("struct Request"));
        assert!(pkg.header.contains("struct Response"));
        assert!(pkg.header.contains("int32_t a"));
        assert!(pkg.header.contains("int32_t sum"));

        // FFI files
        assert!(pkg.request_ffi_rs.contains("#[repr(C)]"));
        assert!(pkg.response_ffi_rs.contains("#[repr(C)]"));
    }

    #[test]
    fn test_cpp_simple_action_generation() {
        let action =
            parse_action("int32 order\n---\nint32 result_code\n---\nint32 progress\n").unwrap();
        let type_hash = "TypeHashNotSupported";

        let result =
            generate_cpp_action_package("example_interfaces", "Fibonacci", &action, type_hash);
        assert!(result.is_ok());

        let pkg = result.unwrap();

        // Header checks
        assert!(
            pkg.header
                .contains("#ifndef EXAMPLE_INTERFACES_ACTION_FIBONACCI_HPP")
        );
        assert!(
            pkg.header
                .contains("namespace example_interfaces { namespace action {")
        );
        assert!(pkg.header.contains("struct Fibonacci"));
        assert!(pkg.header.contains("struct Goal"));
        assert!(pkg.header.contains("struct Result"));
        assert!(pkg.header.contains("struct Feedback"));
        assert!(pkg.header.contains("int32_t order"));
        assert!(pkg.header.contains("int32_t result_code"));
        assert!(pkg.header.contains("int32_t progress"));

        // FFI files
        assert!(pkg.goal_ffi_rs.contains("#[repr(C)]"));
        assert!(pkg.result_ffi_rs.contains("#[repr(C)]"));
        assert!(pkg.feedback_ffi_rs.contains("#[repr(C)]"));
    }
}
