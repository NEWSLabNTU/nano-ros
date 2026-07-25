//! phase-304 W4 — offline Tier-A fixture test (runs in CI, NO ROS runtime).
//!
//! The committed golden hashes in
//! `packages/testing/nros-tests/fixtures/ros-editions/jazzy/{hashes.txt,srv-hashes.txt}`
//! were captured byte-for-byte from a live Jazzy container (W4 capture script).
//! This test drives them as DATA: for every fixture line whose type nano-ros
//! knows how to construct, it asserts the `rosidl_codegen::rihs` engine
//! reproduces the captured hash. Re-capturing the fixtures updates the expected
//! values automatically. No ROS install / container required.
//!
//! The live-container half (fixture == what Jazzy emits TODAY, a drift guard)
//! lives in `edition_hash_oracle.rs` (docker-gated). engine == fixture (here) +
//! fixture == live (there) ⟹ engine == live.

use rosidl_parser::ast::{Field, FieldType as Ast, Message, PrimitiveType};
use std::{collections::HashMap, path::PathBuf};

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testing/nros-tests/fixtures/ros-editions/jazzy")
}

/// Parse a `<type_name> <hash-or-MISSING>` fixture file into a map, dropping
/// comment (`#`) and `MISSING`/`MISSING(...)` lines.
fn load_fixture(name: &str) -> HashMap<String, String> {
    let path = fixtures_dir().join(name);
    let body = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read fixture {}: {e}", path.display()));
    let mut map = HashMap::new();
    for line in body.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut it = line.split_whitespace();
        let (Some(ty), Some(hash)) = (it.next(), it.next()) else {
            continue;
        };
        if hash.starts_with("RIHS01_") {
            map.insert(ty.to_string(), hash.to_string());
        }
    }
    map
}

fn field(name: &str, ft: Ast) -> Field {
    Field {
        field_type: ft,
        name: name.to_string(),
        default_value: None,
    }
}
fn msg(fields: Vec<Field>) -> Message {
    Message {
        fields,
        constants: vec![],
    }
}
fn prim(p: PrimitiveType) -> Ast {
    Ast::Primitive(p)
}
fn nested(pkg: &str, name: &str) -> Ast {
    Ast::NamespacedType {
        package: Some(pkg.to_string()),
        name: name.to_string(),
    }
}

// --- The type definitions nano-ros can reconstruct offline (as parsed .msg). ---

fn time_msg() -> Message {
    msg(vec![
        field("sec", prim(PrimitiveType::Int32)),
        field("nanosec", prim(PrimitiveType::UInt32)),
    ])
}
fn vector3_msg() -> Message {
    msg(vec![
        field("x", prim(PrimitiveType::Float64)),
        field("y", prim(PrimitiveType::Float64)),
        field("z", prim(PrimitiveType::Float64)),
    ])
}
fn quaternion_msg() -> Message {
    msg(vec![
        field("x", prim(PrimitiveType::Float64)),
        field("y", prim(PrimitiveType::Float64)),
        field("z", prim(PrimitiveType::Float64)),
        field("w", prim(PrimitiveType::Float64)),
    ])
}

/// A resolver over the offline-known nested types (Time / Vector3 / Quaternion).
fn offline_resolve(fqn: &str) -> Option<Message> {
    match fqn {
        "builtin_interfaces/msg/Time" => Some(time_msg()),
        "geometry_msgs/msg/Vector3" => Some(vector3_msg()),
        "geometry_msgs/msg/Quaternion" => Some(quaternion_msg()),
        _ => None,
    }
}

fn hash_msg(fqn: &str, m: &Message) -> String {
    let d = rosidl_codegen::rihs::build_type_description(fqn, m, offline_resolve)
        .unwrap_or_else(|e| panic!("build {fqn}: {e}"));
    rosidl_codegen::rihs::rihs01(&d)
}

#[test]
fn engine_reproduces_committed_msg_fixtures() {
    let fx = load_fixture("hashes.txt");

    // The message types nano-ros can reconstruct from a parsed .msg AST.
    let int32 = msg(vec![field("data", prim(PrimitiveType::Int32))]);
    let header = msg(vec![
        field("stamp", nested("builtin_interfaces", "Time")),
        field("frame_id", Ast::String),
    ]);
    let twist = msg(vec![
        field("linear", nested("geometry_msgs", "Vector3")),
        field("angular", nested("geometry_msgs", "Vector3")),
    ]);

    let cases: Vec<(&str, String)> = vec![
        ("std_msgs/msg/Int32", hash_msg("std_msgs/msg/Int32", &int32)),
        (
            "std_msgs/msg/Header",
            hash_msg("std_msgs/msg/Header", &header),
        ),
        (
            "geometry_msgs/msg/Twist",
            hash_msg("geometry_msgs/msg/Twist", &twist),
        ),
    ];

    let mut covered = 0;
    for (ty, got) in &cases {
        let want = fx
            .get(*ty)
            .unwrap_or_else(|| panic!("fixture hashes.txt missing {ty}"));
        assert_eq!(got, want, "engine hash for {ty} != captured Jazzy fixture");
        covered += 1;
    }
    assert!(
        covered >= 3,
        "expected >=3 covered msg fixtures, got {covered}"
    );
}

#[test]
fn engine_reproduces_committed_service_fixtures() {
    let fx = load_fixture("srv-hashes.txt");

    // std_srvs/srv/SetBool — Request=bool, Response=bool+string.
    let request = msg(vec![field("data", prim(PrimitiveType::Bool))]);
    let response = msg(vec![
        field("success", prim(PrimitiveType::Bool)),
        field("message", Ast::String),
    ]);
    let no_extra = |_: &str| None;

    let svc = rosidl_codegen::rihs::build_service_type_description(
        "std_srvs", "SetBool", &request, &response, no_extra,
    )
    .unwrap();
    let req = rosidl_codegen::rihs::service_member_type_description(
        "std_srvs", "SetBool", "_Request", &request, no_extra,
    )
    .unwrap();
    let resp = rosidl_codegen::rihs::service_member_type_description(
        "std_srvs",
        "SetBool",
        "_Response",
        &response,
        no_extra,
    )
    .unwrap();

    for (ty, got) in [
        ("std_srvs/srv/SetBool", rosidl_codegen::rihs::rihs01(&svc)),
        (
            "std_srvs/srv/SetBool_Request",
            rosidl_codegen::rihs::rihs01(&req),
        ),
        (
            "std_srvs/srv/SetBool_Response",
            rosidl_codegen::rihs::rihs01(&resp),
        ),
    ] {
        let want = fx
            .get(ty)
            .unwrap_or_else(|| panic!("fixture srv-hashes.txt missing {ty}"));
        assert_eq!(&got, want, "engine hash for {ty} != captured Jazzy fixture");
    }
}
