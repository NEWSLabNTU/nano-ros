//! play_launch phase 91 E10's gate, nano-ros half: an image built from a
//! contract whose settle the `vehicle.braking` extension derives has the
//! same monitors and sizing as one built from the same contract with the
//! settle written by hand.
//!
//! Both models were resolved by `nros-launch-resolve` (play_launch
//! `d9b79f2b`, ros-launch-manifest v0.2.0) from play_launch's
//! `tests/fixtures/contract_ext_vehicle`: `derived.yaml` as written (entry
//! speed 30 km/h, the brake controller's `target_acceleration` -2.5 and
//! `target_jerk` -1.5, a 256 ms response), `written.yaml` with the
//! `vehicle.braking` keys replaced by `duration: 4166.666667ms`; both with
//! `max_age: 100ms` on the detector's `scan` input, so the hazard's guard
//! has a detector this executor observes and the hazard is lowered. The
//! extension is never evaluated here: the model carries the derived settle
//! as a number, with its source beside it.

use nros_orchestration_ir::{contract_monitors, executor_sizing, ftti_budget, mapper_input};
use ros_launch_manifest_model::SystemModel;

fn model(name: &str) -> SystemModel {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/derived_settle")
        .join(name);
    SystemModel::from_yaml_str(&std::fs::read_to_string(&path).unwrap()).expect("model parses")
}

const SAFE_STATE: &str = "/safety/brake_controller/emergency_stop";

#[test]
fn the_derived_settle_is_a_number_with_its_source() {
    let m = model("derived.yaml");
    let ss = m.contracts.node_paths[SAFE_STATE]
        .safe_state
        .clone()
        .unwrap();
    assert!((ss.settle_ms.unwrap() - 4422.667).abs() < 1e-3);
    let src = ss.settle_derived.expect("the source record");
    assert_eq!(src.by, "vehicle.braking");
    let hand = model("written.yaml");
    assert!(
        hand.contracts.node_paths[SAFE_STATE]
            .safe_state
            .as_ref()
            .unwrap()
            .settle_derived
            .is_none()
    );
}

#[test]
fn an_image_from_a_derived_settle_matches_one_written_by_hand() {
    let (d, w) = (model("derived.yaml"), model("written.yaml"));

    let (rows_d, rows_w) = (
        contract_monitors::monitor_rows(&d).unwrap(),
        contract_monitors::monitor_rows(&w).unwrap(),
    );
    assert_eq!(rows_d, rows_w, "monitor rows");
    let (ages_d, ages_w) = (
        contract_monitors::age_rows(&d).unwrap(),
        contract_monitors::age_rows(&w).unwrap(),
    );
    assert_eq!(ages_d, ages_w, "age rows");
    let (haz_d, haz_w) = (
        contract_monitors::hazard_rows(&d, &rows_d, &ages_d),
        contract_monitors::hazard_rows(&w, &rows_w, &ages_w),
    );
    assert_eq!(haz_d, haz_w, "hazard rows");
    assert_eq!(haz_d.len(), 1, "the hazard is lowered: {haz_d:?}");
    // The settle reached the image: the hazard reacts by a scope path, and
    // its row carries the safe state at that path's sink -- the derived
    // 4422.67 ms, rounded -- with that publisher as its sink.
    assert!((4422..=4423).contains(&haz_d[0].settle_ms), "{haz_d:?}");
    let sink = haz_d[0].sink.expect("the sink row");
    assert_eq!(rows_d[sink].fqn, "/safety/brake_controller/brake_cmd");
    assert_ne!(haz_d[0].reaction_id, 0);

    assert_eq!(
        format!("{:?}", ftti_budget::hazard_budgets(&d)),
        format!("{:?}", ftti_budget::hazard_budgets(&w)),
        "FTTI budgets"
    );
    assert_eq!(
        executor_sizing::count_callbacks(&d, |_| true),
        executor_sizing::count_callbacks(&w, |_| true),
        "executor sizing"
    );
    assert_eq!(
        format!("{:?}", mapper_input::rank_from_model(&d)),
        format!("{:?}", mapper_input::rank_from_model(&w)),
        "priority plan"
    );
}
