//! `examples/esp32/rust/talker/` builds via `idf.py` (Phase 212.M.7).
//!
//! The `idf.py set-target esp32c3 && build` runs in the **build stage** — the
//! `esp_idf_talker` fixture (`scripts/build/idf-fixtures.sh`, run by
//! `build-test-fixtures`) produces the ELF. This test asserts the prebuilt ELF
//! rather than running idf.py at run time (issue 0034 / 0041 / AGENTS.md "No
//! compilation inside tests"). Fixture absent (no idf.py / IDF env) → tier-aware
//! skip/fail via the resolver.

#[test]
fn esp32_talker_builds_via_idf_py() -> nros_tests::TestResult<()> {
    let elf =
        nros_tests::fixtures::require_idf_fixture("esp_idf_talker", "build/esp32_bsp_talker.elf")?;
    assert!(
        elf.is_file(),
        "missing esp32 talker ELF at {}",
        elf.display()
    );
    Ok(())
}
