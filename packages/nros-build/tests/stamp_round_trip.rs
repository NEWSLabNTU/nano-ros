use nros_build::stamp::{StampInput, compute_digest, load_stamp, save_stamp};
use std::fs;

#[test]
fn stamp_round_trip() {
    let tmp = tempfile::tempdir().unwrap();
    let f = tmp.path().join("a.msg");
    fs::write(&f, b"int32 x\n").unwrap();
    let inputs = vec![StampInput { path: f }];
    let args = vec!["codegen".into(), "rust".into()];
    let d = compute_digest(&inputs, &args).unwrap();

    let stamp = tmp.path().join(".stamp");
    save_stamp(&stamp, &d).unwrap();
    let loaded = load_stamp(&stamp).unwrap();
    assert_eq!(loaded.as_deref(), Some(d.as_str()));
}

#[test]
fn load_stamp_missing_returns_none() {
    let tmp = tempfile::tempdir().unwrap();
    let stamp = tmp.path().join(".stamp");
    assert!(load_stamp(&stamp).unwrap().is_none());
}
