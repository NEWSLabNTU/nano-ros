//! Phase 234 / issue 0023 regression guard: the opaque-size probe must recover
//! sizes from a **fat-LTO** rlib (bitcode members), so LTO can stay enabled.
//!
//! Builds `nros` with `lto = "fat"` into a throwaway target dir and asserts
//! `extract_sizes` returns non-zero sizes — which, for a bitcode rlib, can only
//! come from the `llvm-nm` name-based fallback (`extract_sizes_via_llvm_nm`),
//! since the `object` ELF byte-size path reads nothing from bitcode. If a future
//! change regresses the fallback (and re-pins `lto = "off"`), this fails.
//!
//! Ignored by default (spawns a ~15s fat-LTO compile + needs the `llvm-tools`
//! component). Run with:
//!   cargo test -p nros-sizes-build --test bitcode_probe -- --ignored

use nros_sizes_build::extract_sizes;
use std::{fs, path::PathBuf, process::Command};

#[test]
#[ignore = "spawns a fat-LTO build of nros"]
fn extract_sizes_recovers_sizes_from_fat_lto_bitcode() {
    // packages/core/nros-sizes-build → packages/core → packages → <repo root>
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("repo root")
        .to_path_buf();
    let target = std::env::temp_dir().join(format!("nros-sizes-fatlto-{}", std::process::id()));

    let status = Command::new(env!("CARGO"))
        .args([
            "build",
            "--release",
            "-p",
            "nros",
            "--no-default-features",
            "--features",
            "rmw-cffi,ffi-size-markers",
        ])
        .env("CARGO_PROFILE_RELEASE_LTO", "fat")
        .env("CARGO_TARGET_DIR", &target)
        .current_dir(&repo)
        .status()
        .expect("spawn cargo build");
    assert!(status.success(), "fat-LTO build of nros failed");

    let deps = target.join("release/deps");
    let rlib = fs::read_dir(&deps)
        .expect("read deps dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .find(|p| {
            let n = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
            n.starts_with("libnros-") && n.ends_with(".rlib")
        })
        .expect("nros rlib");

    // Bitcode rlib → the `object` byte-size path yields nothing → this exercises
    // the llvm-nm name-based fallback (the whole point of keeping LTO on).
    let sizes = extract_sizes(&rlib, "__NROS_SIZE_").expect("extract_sizes");
    let pub_size = *sizes
        .get("PUBLISHER_SIZE")
        .expect("PUBLISHER_SIZE recovered from a fat-LTO bitcode rlib");
    let exec_size = *sizes.get("EXECUTOR_SIZE").expect("EXECUTOR_SIZE recovered");

    assert!(
        pub_size > 0,
        "PUBLISHER_SIZE must be non-zero under fat LTO"
    );
    assert!(
        exec_size > pub_size,
        "EXECUTOR ({exec_size}) should dwarf PUBLISHER ({pub_size}) — sanity"
    );

    let _ = fs::remove_dir_all(&target);
}
