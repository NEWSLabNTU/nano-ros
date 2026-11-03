//! RFC-0118 D1/D5 — a console has two Rust types: the IR's `ConsoleTable`,
//! read from `[board.console]` and `[image.<id>.console]`, and the plan's
//! `Console`, which carries the resolved answer to a runner. `nros-extension`
//! is a publishable crate with no dependency on the IR, so the shape is
//! written twice.
//!
//! The two differ on purpose in two places: the table names a USB id the
//! plan carries as `args.usb` (the bench resolves it to a `port`), and the
//! plan's `port` is resolved, never typed in a table. Every other key must
//! cross under the same name and the same JSON form, so a console fact added
//! to the table cannot be dropped on its way to the runner.

use std::collections::BTreeSet;

use nros_cli_core::nros_extension::plan::Console as Wire;
use nros_orchestration_ir::target_exec::ConsoleTable as Ir;

/// Table keys the plan carries elsewhere, with where.
const CARRIED_ELSEWHERE: &[(&str, &str)] = &[("usb", "args.usb")];

/// Plan keys no table states.
const RESOLVED_ONLY: &[&str] = &["port"];

/// A table with every key set.
const FULL_TABLE: &str =
    r#"{"kind":"serial","usb":"0483:374b","baud":115200,"command":"picocom -b {baud} {port}"}"#;

/// A plan console with every key set.
const FULL_WIRE: &str =
    r#"{"kind":"serial","port":"/dev/ttyACM0","baud":115200,"command":"picocom -b {baud} {port}"}"#;

fn keys(v: &serde_json::Value) -> BTreeSet<String> {
    v.as_object().unwrap().keys().cloned().collect()
}

#[test]
fn every_table_key_reaches_the_plan() {
    let ir: Ir = serde_json::from_str(FULL_TABLE).expect("the IR reads a full table");
    let wire: Wire = serde_json::from_str(FULL_WIRE).expect("the plan reads a full console");
    let ir_keys = keys(&serde_json::to_value(&ir).unwrap());
    let wire_keys = keys(&serde_json::to_value(&wire).unwrap());
    // FULL_TABLE must really be full: a key added to the table type and not
    // to FULL_TABLE would otherwise pass unseen. The table refuses unknown
    // keys, and serde's refusal lists every key it knows.
    let refusal = serde_json::from_str::<Ir>(r#"{"no-such-key":1}"#)
        .unwrap_err()
        .to_string();
    let declared: BTreeSet<String> = refusal
        .split_once("expected one of ")
        .unwrap_or_else(|| panic!("serde's refusal changed shape: {refusal}"))
        .1
        .split(", ")
        .map(|k| k.trim_matches(|c: char| c == '`' || !c.is_ascii() || c.is_whitespace()))
        .map(|k| k.split('`').next().unwrap().to_string())
        .collect();
    assert_eq!(ir_keys, declared, "FULL_TABLE does not set every table key");
    for k in &ir_keys {
        let elsewhere = CARRIED_ELSEWHERE.iter().any(|(t, _)| t == k);
        assert!(
            elsewhere || wire_keys.contains(k),
            "`[board.console] {k}` has no field on the plan's console and no listed home"
        );
    }
    for k in &wire_keys {
        assert!(
            ir_keys.contains(k) || RESOLVED_ONLY.contains(&k.as_str()),
            "the plan's `console.{k}` is in no table and not listed as resolved"
        );
    }
}

#[test]
fn shared_keys_have_one_json_form() {
    let ir: serde_json::Value =
        serde_json::to_value(serde_json::from_str::<Ir>(FULL_TABLE).unwrap()).unwrap();
    let wire: serde_json::Value =
        serde_json::to_value(serde_json::from_str::<Wire>(FULL_WIRE).unwrap()).unwrap();
    for (k, v) in ir.as_object().unwrap() {
        if let Some(w) = wire.get(k) {
            assert_eq!(
                v, w,
                "`{k}` is written differently by the table and the plan"
            );
        }
    }
    // Every console kind reads on both sides under the same word.
    for kind in ["stdio", "serial", "rtt", "semihosting"] {
        let j = format!(r#"{{"kind":"{kind}"}}"#);
        let ir: Ir = serde_json::from_str(&j).unwrap_or_else(|e| panic!("IR refused {j}: {e}"));
        let wire: Wire =
            serde_json::from_str(&j).unwrap_or_else(|e| panic!("plan refused {j}: {e}"));
        assert_eq!(
            serde_json::to_value(ir).unwrap()["kind"],
            serde_json::to_value(wire).unwrap()["kind"],
            "{kind}"
        );
    }
}
