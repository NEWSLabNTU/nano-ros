// build.rs — link search paths for the `fsp` feature.
//
// The `unix-mock` feature is pure Rust + libc and needs nothing here.
// The `fsp` feature links against NVIDIA's `tegra_aon_fsp` static library,
// shipped under SDK Manager EULA. Path is supplied via `NV_SPE_FSP_DIR`.

fn main() {
    println!("cargo:rerun-if-env-changed=NV_SPE_FSP_DIR");

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

    let dir = std::env::var("NV_SPE_FSP_DIR").unwrap_or_else(|_| {
        panic!(
            "nvidia-ivc: feature `fsp` requires NV_SPE_FSP_DIR to point at \
             an installed NVIDIA Orin SPE FSP tree (the directory containing \
             `lib/libtegra_aon_fsp.a`)"
        )
    });

    println!("cargo:rustc-link-search=native={}/lib", dir);
    println!("cargo:rustc-link-lib=static=tegra_aon_fsp");
}
