//! Issue 2702 (interim) -- a C/C++ workspace image whose own
//! `[image.<id>] features` / `node_interfaces` differ from `[system]` is
//! refused, never silently built with the `[system]` list.
//!
//! Every C/C++ image on one cmake coordinate is configured together, and that
//! configure builds ONE runtime with the `[system]` features, so an image's own
//! choice is not carried. The verdict is recorded at build time by the
//! `image_features_shared_configure_refused` row (the refusal is the `nros`
//! CLI's, before any cmake).

#[test]
fn an_image_feature_choice_a_shared_configure_cannot_carry_is_refused() {
    let stamp =
        nros_tests::fixtures::require_compile_check("image_features_shared_configure_refused")
            .unwrap_or_else(|e| panic!("{e}"));
    let dir = stamp.parent().expect("the row dir");
    let exit = std::fs::read_to_string(dir.join("exit")).expect("the recorded exit");
    let err = std::fs::read_to_string(dir.join("stderr")).expect("the recorded stderr");
    assert_ne!(
        exit.trim(),
        "0",
        "the build must refuse; it exited 0:\n{err}"
    );
    let err = err.split_whitespace().collect::<Vec<_>>().join(" ");
    for want in [
        "`probe_bringup:slim` drops",
        "`rosout`",
        "`probe_bringup:plain`, `probe_bringup:slim`",
        "not carried yet",
    ] {
        assert!(err.contains(want), "the refusal must say {want:?}:\n{err}");
    }
}
