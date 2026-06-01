//! Phase 212.K.4 — user-side Cyclone DDS descriptor codegen.
//!
//! When `--features rmw-cyclonedds` is on, drive the
//! `nros_build::cyclonedds::Descriptors` helper to synthesise the
//! `std_msgs/Int32` Cyclone descriptor + register TU at build time
//! and link the resulting C archive into this binary. Every user
//! payload type lives here — the wrapper crate
//! (`nros-rmw-cyclonedds-sys`) only bakes its own intrinsic
//! discovery descriptor (`rmw_dds_common::ParticipantEntitiesInfo`)
//! and never a user type.
//!
//! No-op under any other RMW feature.

fn main() {
    #[cfg(feature = "rmw-cyclonedds")]
    cyclone::emit();
}

#[cfg(feature = "rmw-cyclonedds")]
mod cyclone {
    pub fn emit() {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
        let emitted = nros_build::cyclonedds::Descriptors::new()
            .msg_search_path(format!("{manifest_dir}/msg"))
            .messages(&["std_msgs/Int32"])
            .emit()
            .expect("nros_build::cyclonedds::Descriptors::emit");

        // The emitted descriptor C pulls `dds/ddsc/dds_public_impl.h`
        // and friends — Cyclone's public + semi-internal headers.
        // `cyclonedds-sys` exports the include path via `DEP_DDSC_INCLUDE`
        // (the standard cargo links-metadata channel).
        let ddsc_include =
            std::env::var("DEP_DDSC_INCLUDE").expect("DEP_DDSC_INCLUDE from cyclonedds-sys");

        let mut cc = cc::Build::new();
        cc.include(&emitted.include_dir).include(&ddsc_include);
        for c in &emitted.generated_c {
            cc.file(c);
        }
        cc.cargo_metadata(false);
        cc.compile("native_rs_listener_cyclonedds_descriptors");

        // `+whole-archive` so the descriptor's
        // `__attribute__((constructor))` register TU survives the
        // final link — same shape the wrapper crate uses for its own
        // intrinsic descriptor (`rmw_dds_common_graph`).
        let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR");
        println!("cargo:rustc-link-search=native={out_dir}");
        println!(
            "cargo:rustc-link-lib=static:+whole-archive,-bundle=\
             native_rs_listener_cyclonedds_descriptors"
        );
    }
}
