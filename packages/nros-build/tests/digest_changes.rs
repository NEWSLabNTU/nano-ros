use nros_build::stamp::{StampInput, compute_digest};
use std::fs;

#[test]
fn digest_changes_when_msg_changes() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("Sensor.msg");
    fs::write(&p, b"int32 id\n").unwrap();
    let inputs = vec![StampInput { path: p.clone() }];
    let args: Vec<String> = vec!["codegen".into(), "rust".into()];

    let before = compute_digest(&inputs, &args).unwrap();

    // Touch contents.
    fs::write(&p, b"int32 id\nfloat32 value\n").unwrap();
    let after = compute_digest(&inputs, &args).unwrap();

    assert_ne!(before, after, "digest must change with file content");
}

#[test]
fn digest_changes_when_args_change() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("Sensor.msg");
    fs::write(&p, b"int32 id\n").unwrap();
    let inputs = vec![StampInput { path: p }];

    let a1: Vec<String> = vec!["codegen".into(), "rust".into()];
    let a2: Vec<String> = vec!["codegen".into(), "c".into()];

    let d1 = compute_digest(&inputs, &a1).unwrap();
    let d2 = compute_digest(&inputs, &a2).unwrap();
    assert_ne!(d1, d2);
}
