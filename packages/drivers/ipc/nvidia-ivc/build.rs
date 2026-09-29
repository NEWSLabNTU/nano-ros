// build.rs — link search paths for the `fsp` feature.
//
// The `unix-mock` feature is pure Rust + libc and needs nothing here.
// The `fsp` feature links against NVIDIA's `tegra_aon_fsp` static library,
// shipped under SDK Manager EULA. Path is supplied via `NV_SPE_FSP_DIR`.

fn main() {
    // issue 0491 — `NV_SPE_FSP_DIR` names a DIRECTORY, so its SPELLING is not
    // fingerprinted (cargo compares env values as text, and one directory has
    // several spellings across the entry points that build a leaf). The
    // library it selects is watched by content below.

    let fsp = std::env::var("CARGO_FEATURE_FSP").is_ok();
    let unix_mock = std::env::var("CARGO_FEATURE_UNIX_MOCK").is_ok();
    if fsp && unix_mock {
        panic!(
            "nvidia-ivc: features `fsp` and `unix-mock` are mutually exclusive — \
             pick one (the lib also surfaces this as a compile_error, but \
             build.rs runs first)"
        );
    }
    if !fsp {
        return;
    }

    // issue 1560 site 4 — through `env_path`, the ONE implementation of issue
    // 1280's three-valued rule. The FSP ships under an SDK-Manager EULA and can
    // never be vendored, so this always names a tree outside every nano-ros
    // checkout — the arm `reroot_foreign` deliberately leaves alone, and the
    // reason there is no `just/sdk-env.just` row for it (which is in turn why
    // phase-471 W3's gate cannot see this site). Written anyway because
    // RFC-0101 D3 is a rule about the CALL, not about which value arrives.
    let dir = nros_build_paths::env_path("NV_SPE_FSP_DIR").unwrap_or_else(|| {
        panic!(
            "nvidia-ivc: feature `fsp` requires NV_SPE_FSP_DIR to point at \
             an installed NVIDIA Orin SPE FSP tree (the directory containing \
             `lib/libtegra_aon_fsp.a`)"
        )
    });
    let dir = dir.display();

    let lib = format!("{dir}/lib/libtegra_aon_fsp.a");
    if std::path::Path::new(&lib).exists() {
        // Only when it exists: a `rerun-if-changed` on a missing path is
        // permanently dirty (issue 0490).
        println!("cargo:rerun-if-changed={lib}");
    }
    println!("cargo:rustc-link-search=native={}/lib", dir);
    println!("cargo:rustc-link-lib=static=tegra_aon_fsp");
}
