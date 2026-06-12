//! ESP-IDF 2-component bringup builds via `idf.py` (Phase 212.H.5).
//!
//! The `multi_pkg_workspace_esp_idf` fixture's `esp_idf_app/` builds via
//! `idf.py set-target esp32c3 && build` in the **build stage** — the
//! `esp_idf_bringup` fixture (`scripts/build/idf-fixtures.sh`, run by
//! `build-test-fixtures`) produces the ELF. This test asserts the prebuilt ELF
//! rather than running idf.py at run time (issue 0034 / 0041). Fixture absent
//! (no idf.py / IDF env) → tier-aware skip/fail via the resolver.

#[test]
fn esp_idf_esp32c3_2_component_bringup_builds() -> nros_tests::TestResult<()> {
    let elf = nros_tests::fixtures::require_idf_fixture(
        "esp_idf_bringup",
        "esp_idf_app/build/multi_pkg_workspace_esp_idf.elf",
    )?;
    assert!(
        elf.is_file(),
        "missing 2-component bringup ELF at {}",
        elf.display()
    );
    Ok(())
}
