//! §212.L.9 cmake-fn metadata — `nano_ros_node_register` emits
//! `nros-metadata.json` with the expected component entries.
//! (`nano_ros_deploy` + its `l9_deploy` fixture were retired post-287 —
//! the deploy/rmw tuple lives in package.xml.)
//!
//! The cmake configure runs in the **build stage** — two cmake fixtures
//! (`l9_register_cpp`, `l9_register_c` under
//! `compile-check-fixtures.sh`) each configure a tiny CMakeLists exercising one
//! cmake fn and emit `nros-metadata.json`. These tests inspect the prebuilt JSON
//! rather than running cmake at run time (issue 0034 / 0041). The negative
//! reject-diagnostic cases (the configure must FAIL) live in
//! `cmake_node_register_misuse.rs` (a documented exception).

fn metadata(id: &str) -> nros_tests::TestResult<String> {
    let p = nros_tests::fixtures::require_cmake_fixture(id, "nros-metadata.json")?;
    Ok(std::fs::read_to_string(&p).expect("read nros-metadata.json"))
}

#[test]
fn nano_ros_node_register_emits_metadata() -> nros_tests::TestResult<()> {
    let body = metadata("l9_register_cpp")?;
    assert!(
        body.contains("\"name\": \"talker\"") && body.contains("\"class\": \"talker_pkg::Talker\""),
        "metadata missing component entry:\n{body}"
    );
    assert!(
        body.contains("\"sources\": [\"src/dummy.cpp\"]"),
        "metadata sources mismatch:\n{body}"
    );
    assert!(
        body.contains("\"deploy\": [\"native\", \"zephyr\"]"),
        "metadata deploy mismatch:\n{body}"
    );
    assert!(
        body.contains("\"lang\": \"cpp\""),
        "metadata lang mismatch:\n{body}"
    );
    assert!(
        body.to_lowercase().contains("\"pkg_dir\""),
        "metadata missing pkg_dir field:\n{body}"
    );
    Ok(())
}

#[test]
fn nano_ros_node_register_accepts_c_language() -> nros_tests::TestResult<()> {
    let body = metadata("l9_register_c")?;
    assert!(
        body.contains("\"class\": \"c_talker_pkg::Talker\""),
        "metadata class mismatch:\n{body}"
    );
    assert!(
        body.contains("\"sources\": [\"src/dummy.c\"]"),
        "metadata sources mismatch:\n{body}"
    );
    assert!(
        body.contains("\"lang\": \"c\""),
        "metadata lang mismatch:\n{body}"
    );
    Ok(())
}

