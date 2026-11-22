//! Issue 2702 (ruled 2026-11-22) -- every C/C++ workspace image gets a cmake
//! configure of its own, built and sized with exactly its own capability axes:
//! an image stating `node_interfaces = "none"` drops the stock interfaces from
//! its build and its descriptor, and the image beside it keeps them.
//!
//! Recorded at build time by the `image_features_shared_configure_refused`
//! row (the name predates the ruling): `nros build` of each image of a
//! two-image `linux` workspace, `slim` stating `node_interfaces = "none"`.

use std::path::Path;

fn read(dir: &Path, image: &str, file: &str) -> String {
    std::fs::read_to_string(dir.join(image).join(file))
        .unwrap_or_else(|e| panic!("{image}/{file}: {e}"))
}

fn built(dir: &Path, image: &str) {
    let exit = read(dir, image, "exit");
    assert_eq!(
        exit.trim(),
        "0",
        "`{image}` must build:\n{}",
        read(dir, image, "stderr")
    );
}

#[test]
fn an_image_with_its_own_capability_axes_gets_its_own_configure() {
    let stamp =
        nros_tests::fixtures::require_compile_check("image_features_shared_configure_refused")
            .unwrap_or_else(|e| panic!("{e}"));
    let dir = stamp.parent().expect("the row dir");
    built(dir, "plain");
    built(dir, "slim");

    let plain_dir = read(dir, "plain", "build_dir");
    let slim_dir = read(dir, "slim", "build_dir");
    assert_ne!(
        plain_dir.trim(),
        slim_dir.trim(),
        "the two images must not share a configure"
    );
    for (image, d) in [("plain", &plain_dir), ("slim", &slim_dir)] {
        assert!(
            d.trim().ends_with(&format!("-probe_bringup-{image}")),
            "`{image}`'s configure is its own, named for it: {d}"
        );
    }

    // Each configure builds with its image's own axes...
    let plain_features = read(dir, "plain", "features");
    for f in ["param_services", "rosout"] {
        assert!(
            plain_features.contains(f),
            "`plain` builds {f}: {plain_features}"
        );
    }
    assert_eq!(
        read(dir, "slim", "features").trim(),
        "NROS_FEATURES:STRING=",
        "`slim` opts out of the stock node interfaces"
    );
    // ...and is sized with them.
    let plain = read(dir, "plain", "descriptor.toml");
    let slim = read(dir, "slim", "descriptor.toml");
    assert!(
        plain.contains(r#"infra_families = "param+rosout+clock""#),
        "{plain}"
    );
    assert!(plain.contains(r#"store = "param_services""#), "{plain}");
    assert!(slim.contains(r#"infra_families = "none""#), "{slim}");
    assert!(slim.contains(r#"store = "none""#), "{slim}");
    assert!(slim.contains("publisher_count = 0"), "{slim}");
}
