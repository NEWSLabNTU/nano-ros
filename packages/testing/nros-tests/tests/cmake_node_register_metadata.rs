//! §212.L.9 cmake-fn metadata — `nano_ros_node_register` / `nano_ros_deploy`
//! emit `nros-metadata.json` with the expected component / deploy entries.
//!
//! The cmake configure runs in the **build stage** — three cmake fixtures
//! (`l9_register_cpp`, `l9_register_c`, `l9_deploy` under
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

#[test]
fn nano_ros_deploy_records_target_config() -> nros_tests::TestResult<()> {
    let body = metadata("l9_deploy")?;
    assert!(
        body.contains("\"native\": {\"rmw\": \"zenoh\", \"domain_id\": 7, \"locator\": null}"),
        "missing native deploy_targets entry:\n{body}"
    );
    assert!(
        body.contains(
            "\"zephyr\": {\"rmw\": \"cyclonedds\", \"domain_id\": 3, \"locator\": \"tcp/10.0.0.1:7447\"}"
        ),
        "missing zephyr deploy_targets entry:\n{body}"
    );
    Ok(())
}
