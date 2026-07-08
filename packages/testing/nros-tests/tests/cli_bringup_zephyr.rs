//! `nros codegen-system` adapter shim → baked `system_config.h` → native_sim
//! ELF boots (Phase 212.H.1; `system_main.c` retired in phase-258 — issue 0154).
//!
//! The native_sim `west build` of `multi_pkg_workspace_zephyr/zephyr_app` runs in
//! the **build stage** — the `west_bringup_zephyr` west fixture
//! (`scripts/build/west-fixtures.sh`, run by `just zephyr build-fixtures`). This
//! test inspects the prebuilt baked artifacts (`nros-system/system_config.h`,
//! `system_config.cmake`) and boots the prebuilt `zephyr.exe` rather than running west
//! at run time (issue 0034 / 0041). Fixture absent (no west / Zephyr workspace) →
//! tier-aware skip/fail via the resolver.

use std::time::Duration;

#[test]
fn cli_bringup_zephyr_adapter_shim_boots_native_sim() -> nros_tests::TestResult<()> {
    let elf =
        nros_tests::fixtures::require_west_fixture("west_bringup_zephyr", "zephyr/zephyr.exe")?;
    let build_dir = elf.parent().and_then(|p| p.parent()).expect("build dir");

    // Phase 212.E baked artifacts.
    let baked = build_dir.join("nros-system");
    assert!(
        baked.join("system_config.h").exists(),
        "baked system_config.h missing under {}",
        baked.display()
    );
    // Issue 0154 — `system_main.c` retired in phase-258 (install-seam
    // registration); the cmake-side mirror completes the bake contract.
    assert!(
        baked.join("system_config.cmake").exists(),
        "baked system_config.cmake missing under {}",
        baked.display()
    );

    // Boot the prebuilt ELF.
    assert!(elf.exists(), "zephyr.exe missing at {}", elf.display());
    let mut proc = nros_tests::zephyr::ZephyrProcess::start(
        &elf,
        nros_tests::zephyr::ZephyrPlatform::NativeSim,
    )
    .expect("spawn zephyr native_sim ELF");
    let output = proc
        .wait_for_output(Duration::from_secs(2))
        .unwrap_or_default();
    eprintln!("--- native_sim stdout ---\n{output}\n--- end ---");
    // 212.H.1 scope is the adapter-shim contract: config baked + compiled into
    // the app's stub main, ELF boots in native_sim and echoes the baked values.
    assert!(
        output.contains("nros adapter shim:"),
        "native_sim ELF did not print the baked-config boot line — shim contract broken.\nOutput:\n{output}"
    );
    Ok(())
}
