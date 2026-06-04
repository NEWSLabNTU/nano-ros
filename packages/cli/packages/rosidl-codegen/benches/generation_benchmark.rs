use criterion::{Criterion, criterion_group, criterion_main};
use rosidl_codegen::{generate_action_package, generate_message_package, generate_service_package};
use rosidl_parser::{parse_action, parse_message, parse_service};
use std::{collections::HashSet, hint::black_box};

fn benchmark_simple_message(c: &mut Criterion) {
    let msg_def = "int32 x\nfloat64 y\nstring name\n";
    let msg = parse_message(msg_def).unwrap();
    let deps = HashSet::new();

    c.bench_function("generate_simple_message", |b| {
        b.iter(|| {
            generate_message_package(
                black_box("test_msgs"),
                black_box("Point2D"),
                black_box(&msg),
                black_box(&deps),
            )
        });
    });
}

fn benchmark_message_with_arrays(c: &mut Criterion) {
    let msg_def = "int32[5] small_array\nfloat64[100] large_array\nint32[] sequence\n";
    let msg = parse_message(msg_def).unwrap();
    let deps = HashSet::new();

    c.bench_function("generate_message_with_arrays", |b| {
        b.iter(|| {
            generate_message_package(
                black_box("test_msgs"),
                black_box("ArrayMsg"),
                black_box(&msg),
                black_box(&deps),
            )
        });
    });
}

fn benchmark_complex_message(c: &mut Criterion) {
    // Simulates sensor_msgs/Image structure
    let msg_def = r#"std_msgs/Header header
uint32 height
uint32 width
string encoding
uint8 is_bigendian
uint32 step
uint8[] data
"#;
    let msg = parse_message(msg_def).unwrap();
    let deps = HashSet::new();

    c.bench_function("generate_complex_message", |b| {
        b.iter(|| {
            generate_message_package(
                black_box("sensor_msgs"),
                black_box("Image"),
                black_box(&msg),
                black_box(&deps),
            )
        });
    });
}

fn benchmark_simple_service(c: &mut Criterion) {
    let srv_def = "int32 a\nint32 b\n---\nint32 sum\n";
    let srv = parse_service(srv_def).unwrap();
    let deps = HashSet::new();

    c.bench_function("generate_simple_service", |b| {
        b.iter(|| {
            generate_service_package(
                black_box("example_interfaces"),
                black_box("AddTwoInts"),
                black_box(&srv),
                black_box(&deps),
            )
        });
    });
}

fn benchmark_simple_action(c: &mut Criterion) {
    let action_def = "int32 order\n---\nint32[] sequence\n---\nint32[] partial_sequence\n";
    let action = parse_action(action_def).unwrap();
    let deps = HashSet::new();

    c.bench_function("generate_simple_action", |b| {
        b.iter(|| {
            generate_action_package(
                black_box("example_interfaces"),
                black_box("Fibonacci"),
                black_box(&action),
                black_box(&deps),
            )
        });
    });
}

fn benchmark_message_with_dependencies(c: &mut Criterion) {
    let msg_def = "geometry_msgs/Point position\ngeometry_msgs/Quaternion orientation\n";
    let msg = parse_message(msg_def).unwrap();
    let deps = HashSet::new();

    c.bench_function("generate_message_with_dependencies", |b| {
        b.iter(|| {
            generate_message_package(
                black_box("geometry_msgs"),
                black_box("Pose"),
                black_box(&msg),
                black_box(&deps),
            )
        });
    });
}

criterion_group!(
    benches,
    benchmark_simple_message,
    benchmark_message_with_arrays,
    benchmark_complex_message,
    benchmark_simple_service,
    benchmark_simple_action,
    benchmark_message_with_dependencies
);
criterion_main!(benches);
