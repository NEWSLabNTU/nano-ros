//! RFC-0118 D7 — a `prepare` step has two Rust types and one JSON form.
//!
//! `nros_orchestration_ir::target_exec::PrepareStep` parses the board and
//! image tables a plan is resolved from; `nros_extension::plan::PrepareStep`
//! is what the plan carries to a runner. `nros-extension` is a publishable
//! crate with no dependency on the IR, so the shape is written twice. This
//! test holds the two equal in both directions, so a fourth step kind or a
//! fifth `wait_for` event cannot land on one side only.

use nros_cli_core::nros_extension::plan::PrepareStep as Wire;
use nros_orchestration_ir::target_exec::PrepareStep as Ir;

/// Every step kind and every `wait_for` event, as JSON.
const STEPS: &[&str] = &[
    r#"{"say":"Set BOOT0 to 1"}"#,
    r#"{"wait_for":"usb:0483:df11"}"#,
    r#"{"wait_for":"port:/dev/ttyACM0"}"#,
    r#"{"wait_for":"volume:RPI-RP2"}"#,
    r#"{"wait_for":"line:READY"}"#,
    r#"{"say":"Press RESET","wait_for":"usb:2e8a:0003"}"#,
    r#"{"run":"./relay.sh 3"}"#,
];

/// Shapes both sides must refuse.
const REFUSED: &[&str] = &[
    r#"{}"#,
    r#"{"run":""}"#,
    r#"{"say":"x","run":"y"}"#,
    r#"{"wait_for":"gpio:3"}"#,
    r#"{"wait_for":"usb:0483"}"#,
    r#"{"say":"x","extra":1}"#,
];

#[test]
fn both_types_read_and_write_the_same_json() {
    for json in STEPS {
        let ir: Ir =
            serde_json::from_str(json).unwrap_or_else(|e| panic!("IR refused {json}: {e}"));
        let wire: Wire =
            serde_json::from_str(json).unwrap_or_else(|e| panic!("wire refused {json}: {e}"));
        let from_ir = serde_json::to_string(&ir).unwrap();
        let from_wire = serde_json::to_string(&wire).unwrap();
        assert_eq!(from_ir, from_wire, "the two types write {json} differently");
        // And each reads what the other writes.
        assert!(serde_json::from_str::<Wire>(&from_ir).is_ok(), "{from_ir}");
        assert!(
            serde_json::from_str::<Ir>(&from_wire).is_ok(),
            "{from_wire}"
        );
    }
}

#[test]
fn both_types_refuse_the_same_shapes() {
    for json in REFUSED {
        assert!(
            serde_json::from_str::<Ir>(json).is_err(),
            "IR accepted {json}"
        );
        assert!(
            serde_json::from_str::<Wire>(json).is_err(),
            "wire accepted {json}"
        );
    }
}
