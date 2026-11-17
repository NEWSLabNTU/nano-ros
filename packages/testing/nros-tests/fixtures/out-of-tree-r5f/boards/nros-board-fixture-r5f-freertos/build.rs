//! Board glue for `fixture-r5f-freertos`: nano-ros's FreeRTOS overlay runner,
//! plus the vendor SDK's sources and header from FIXTURE_R5F_SDK_ROOT.
use nros_board_common::freertos_build::{Overlay, OverlayEnv, run_overlay};

fn main() {
    // The SDK root's CONTENT is watched, never its spelling (issue 0491).
    let sdk = nros_build_paths::env_path_watched("FIXTURE_R5F_SDK_ROOT").unwrap_or_default();
    let sdk_include = sdk.join("include");
    let sdk_src = sdk.join("src/fxsdk.c");
    let glue = move |_: &OverlayEnv, b: &mut cc::Build| {
        assert!(
            sdk_include.join("fixture_r5f_sdk.h").is_file(),
            "fixture-r5f-freertos: FIXTURE_R5F_SDK_ROOT does not name the board's vendor SDK"
        );
        b.include(&sdk_include);
        b.file(&sdk_src);
        nros_build_paths::watch_path(&sdk_src);
    };
    run_overlay(&Overlay {
        configure_glue: Some(&glue),
        ..Overlay::new(
            "nros-board-fixture-r5f-freertos",
            "fixture_r5f.ld",
            "GCC_ARM_CRX_NOGIC",
            &["c/board_fixture_r5f.c"],
        )
    });
}
